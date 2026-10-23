//! `ika serve`: KB をブラウザで読むためのローカルサーバー。
//! 読み取り専用（GET・HEAD のみ）で、127.0.0.1 だけで待ち受ける。
//! ファイルは毎回読み直すので、`ika` で書いた内容は再読み込みですぐ見える。

use std::io::Write;
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::thread;

use anyhow::{Context, Result};

use crate::kb;

mod datasets;
mod highlight;
mod http;
mod page;
mod tags;

use http::{Request, Response};

#[derive(clap::Args)]
pub struct Args {
    /// 待ち受けるポート（0 なら空いているポートを選ぶ）
    #[arg(long, default_value_t = 8787)]
    port: u16,
    /// 起動したらブラウザで開く
    #[arg(long)]
    open: bool,
}

pub fn run(root: &Path, args: Args) -> Result<()> {
    // 127.0.0.1 だけ。ほかのマシンからは届かない
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, args.port)).with_context(|| {
        format!(
            "127.0.0.1:{} で待ち受けられません（使用中なら --port で変えてください）",
            args.port
        )
    })?;
    let port = listener.local_addr()?.port();
    let url = format!("http://127.0.0.1:{port}/");
    println!("{url}");
    println!(
        "KB: {}（読み取り専用・このマシンからだけ。Ctrl-C で終了）",
        root.display()
    );
    std::io::stdout().flush()?;
    if args.open {
        open_browser(&url);
    }

    let root = Arc::new(root.to_path_buf());
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let root = Arc::clone(&root);
        thread::spawn(move || serve_one(stream, &root, port));
    }
    Ok(())
}

pub(crate) fn open_browser(url: &str) {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    if Command::new(opener).arg(url).spawn().is_err() {
        eprintln!("ブラウザを開けませんでした。上の URL を開いてください");
    }
}

fn serve_one(mut stream: TcpStream, root: &Path, port: u16) {
    let (response, head_only) = match http::read_request(&mut stream) {
        Ok(request) => (handle(root, port, &request), request.method == "HEAD"),
        Err(status) => (error(status, "リクエストを読めません"), false),
    };
    let _ = http::write_response(&mut stream, &response, head_only);
}

fn error(status: u16, message: &str) -> Response {
    Response::html(status, page::error(status, message))
}

fn handle(root: &Path, port: u16, request: &Request) -> Response {
    // DNS リバインディング対策: ブラウザが別名で名乗ってきたリクエストは受けない
    let host_ok = request.host.as_deref().is_some_and(|host| {
        let host = host.to_ascii_lowercase();
        host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}")
    });
    if !host_ok {
        return error(
            403,
            "この Host ではアクセスできません（127.0.0.1 か localhost で開いてください）",
        );
    }
    if request.method != "GET" && request.method != "HEAD" {
        return error(405, "読み取り専用です（GET だけ受け付けます）");
    }
    route(root, request).unwrap_or_else(|err| error(500, &format!("内部エラー: {err:#}")))
}

fn route(root: &Path, request: &Request) -> Result<Response> {
    let path = request.path.as_str();
    let html = |body: String| Response::html(200, body);
    Ok(match path {
        "/" => html(page::index(root)?),
        "/search" => html(page::search_page(root, request)?),
        "/tags" | "/tags/" => html(tags::index(root, request)?),
        "/kb" => kb_page(root, "kb", request)?,
        _ => {
            if let Some(name) = path.strip_prefix("/data/") {
                match datasets::page(root, name.trim_end_matches('/'))? {
                    Some(body) => html(body),
                    None => error(404, "そのデータセットはありません"),
                }
            } else if let Some(tag) = path.strip_prefix("/tags/") {
                match tags::page(root, tag)? {
                    Some(body) => html(body),
                    None => error(404, "そのタグの付いたページはありません"),
                }
            } else if let Some(name) = path.strip_prefix("/project/") {
                match page::project_page(root, name.trim_end_matches('/'))? {
                    Some(body) => html(body),
                    None => error(404, "そのプロダクトは登録されていません"),
                }
            } else if let Some(rest) = path.strip_prefix("/kb/") {
                kb_page(root, &format!("kb/{}", rest.trim_end_matches('/')), request)?
            } else {
                error(404, "ページがありません")
            }
        }
    })
}

/// `kb/...` のディレクトリ・Markdown・画像。Markdown は `?hl=` の語をハイライトする
fn kb_page(root: &Path, requested: &str, request: &Request) -> Result<Response> {
    let Ok(rel) = kb::resolve(root, requested) else {
        return Ok(error(400, "不正なパスです"));
    };
    if kb::guard_symlink(root, &rel).is_err() {
        return Ok(error(403, "シンボリックリンクは辿れません"));
    }
    let abs: PathBuf = root.join(&rel);
    if abs.is_dir() {
        return Ok(Response::html(200, page::directory(root, &rel)?));
    }
    if !abs.is_file() {
        return Ok(error(404, "ファイルがありません"));
    }
    let ext = abs
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let image = match ext.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    };
    Ok(match (ext.as_str(), image) {
        ("md", _) => {
            let text = std::fs::read_to_string(&abs)?;
            let needles = highlight::parse(request.param("hl"));
            Response::html(200, page::markdown_page(root, &rel, &text, &needles)?)
        }
        (_, Some(content_type)) => Response::bytes(content_type, std::fs::read(&abs)?),
        _ => error(
            404,
            "表示できるのは Markdown と画像（png・jpg・gif・webp）だけです",
        ),
    })
}
