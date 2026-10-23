//! `ika serve` のページ。ユーザー・KB 由来の文字列は必ず `esc` を通して埋め込む。

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

use super::highlight;
use super::http::{Request, percent_decode, percent_encode};
use crate::kb::{self, fts, search};
use crate::registry::{self, Project};
use crate::render::{self, Href, RenderOptions, is_web_url};
use crate::{check, frontmatter, project};

pub use crate::render::esc;

/// 日本語の Web フォント（Google Fonts の CDN）。`display=swap` で、読み込めるまでは次のフォントで表示する
const FONT_CSS_URL: &str =
    "https://fonts.googleapis.com/css2?family=Noto+Sans+JP:wght@400;500;700&display=swap";

const CSS: &str = r#"
/* 色は ika-diagram の変数（ika export html と同じ）を使う。ライト・ダークもそちらで切り替わる */
:root { --bg:var(--paper); --fg:var(--ink); --muted:var(--ink-2); --soft:var(--raise); --link:var(--accent); }
* { box-sizing: border-box; }
body { margin:0; background:var(--bg); color:var(--fg); letter-spacing:.01em;
  font:17px/1.9 "Noto Sans JP", -apple-system, BlinkMacSystemFont, "Hiragino Sans", "Hiragino Kaku Gothic ProN", "Yu Gothic", "Meiryo", "Segoe UI", sans-serif; }
code, pre { font-family:ui-monospace, "SF Mono", Menlo, Consolas, "Noto Sans Mono", monospace; letter-spacing:0; }
a { color:var(--link); text-decoration:none; } a:hover { text-decoration:underline; }
header { border-bottom:1px solid var(--line); background:var(--soft); }
header .bar { max-width:52rem; margin:0 auto; padding:.6rem 1rem; display:flex; gap:1rem; align-items:center; }
header .logo { font-weight:700; color:var(--fg); }
header form { margin-left:auto; display:flex; gap:.4rem; }
input, select, button { font:inherit; padding:.25rem .5rem; border:1px solid var(--line); border-radius:6px;
  background:var(--bg); color:var(--fg); }
main { max-width:52rem; margin:0 auto; padding:1rem 1rem 4rem; }
h1, h2, h3 { font-weight:700; line-height:1.45; font-feature-settings:"palt"; }
h1 { font-size:1.75rem; margin:1.2rem 0 .5rem; } h2 { font-size:1.35rem; margin:2.2rem 0 .6rem; border-bottom:1px solid var(--line); padding-bottom:.25rem; }
h3 { font-size:1.12rem; margin:1.6rem 0 .4rem; }
article { max-width:46rem; line-break:strict; overflow-wrap:anywhere; text-wrap:pretty; }
article p { margin:1em 0; } article li { margin:.3em 0; } article h2:first-child { margin-top:1rem; }
.crumbs, .meta, .muted { color:var(--muted); font-size:.88rem; line-height:1.7; }
.badge { display:inline-block; padding:0 .5rem; border-radius:1rem; font-size:.78rem; border:1px solid var(--line); background:var(--soft); margin-right:.3rem; }
.badge.decision { border-color:var(--accent); color:var(--accent); }
.badge.insight { border-color:var(--link); color:var(--link); }
.badge.spec { border-color:var(--accent); }
ul.list { list-style:none; padding:0; margin:.5rem 0; }
ul.list li { padding:.35rem 0; border-bottom:1px solid var(--line); }
ul.list .path { display:block; color:var(--muted); font-size:.8rem; word-break:break-all; }
.snippet { color:var(--muted); font-size:.85rem; margin:.1rem 0 0 1rem; word-break:break-all; }
article pre { background:var(--soft); border:1px solid var(--line); border-radius:6px; padding:.8rem; overflow-x:auto; }
article code { background:var(--soft); padding:.1rem .3rem; border-radius:4px; font-size:.9em; }
article pre code { background:none; padding:0; }
article table { border-collapse:collapse; display:block; overflow-x:auto; width:max-content;
  max-width:min(calc(50vw + 23rem), calc(100vw - 2.5rem), 84rem); font-size:.9rem; line-height:1.65; }
article th, article td { border:1px solid var(--line); padding:.3rem .7rem; vertical-align:top; overflow-wrap:break-word; }
article th { background:var(--soft); text-align:left; white-space:nowrap; }
.data { overflow-x:auto; width:min(96vw, 90rem); position:relative; left:50%; transform:translateX(-50%); padding:0 .5rem; }
.data table { border-collapse:collapse; font-size:.85rem; line-height:1.6; }
.data th, .data td { border:1px solid var(--line); padding:.25rem .6rem; vertical-align:top; overflow-wrap:normal; word-break:normal; }
.data td { max-width:30rem; }
.data th { position:sticky; top:0; background:var(--soft); text-align:left; white-space:nowrap; }
.data tbody tr:nth-child(even) { background:var(--soft); }
.tags { display:flex; flex-wrap:wrap; gap:.4rem .45rem; list-style:none; padding:0; margin:.6rem 0 1rem; }
.tags a { display:inline-block; padding:0 .65rem; border:1px solid var(--line); border-radius:1rem; background:var(--soft); font-size:.88rem; line-height:1.9; }
.tags a:hover { border-color:var(--link); text-decoration:none; }
.tags .n { color:var(--muted); font-size:.78rem; margin-left:.35rem; }
article blockquote { margin:1rem 0; padding:0 1rem; color:var(--muted); border-left:4px solid var(--line); }
article img { max-width:100%; }
.card { border:1px solid var(--line); border-radius:8px; padding:.6rem 1rem; background:var(--soft); margin:.6rem 0; }
/* 検索語のハイライト。ダークでも読めるよう文字色も固定する */
mark { background:#ffe45c; color:#1f1f1f; border-radius:2px; padding:0 .08em; }
mark#ika-hl { scroll-margin-top:30vh; }
.hl-note { margin:.4rem 0 0; }
form.search { display:flex; flex-wrap:wrap; gap:.4rem; margin:.6rem 0 1rem; }
@media (max-width:40rem) { header .bar { flex-wrap:wrap; } header form { margin-left:0; width:100%; } header input[name=q] { flex:1; } }
"#;

/// ルートからの相対パス（`kb/...`）のページの URL
fn kb_url(rel: &Path) -> String {
    let parts: Vec<_> = rel.iter().map(|p| p.to_string_lossy()).collect();
    format!("/{}", percent_encode(&parts.join("/")))
}

pub(super) fn search_url(key: &str, value: &str) -> String {
    format!("/search?{key}={}", percent_encode(value))
}

/// Web フォントの `<link>` は本文のあとに置く。head に置くと、CDN に届かない（オフライン・遅い）ときに
/// 表示が止まるが、末尾なら本文は先にシステムフォントで描かれ、読み込めたときだけ差し替わる
pub(super) fn layout(title: &str, query: &str, body: &str) -> String {
    format!(
        "<!doctype html><html lang=\"ja\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <title>{}</title><style>{}{}{CSS}</style></head><body>\
         <header><div class=\"bar\"><a class=\"logo\" href=\"/\">ika</a><a href=\"/tags\">タグ</a>\
         <form action=\"/search\" method=\"get\"><input name=\"q\" value=\"{}\" placeholder=\"KB を検索\" size=\"22\">\
         <button>検索</button></form></div></header><main>{body}</main>\
         <link rel=\"stylesheet\" href=\"{FONT_CSS_URL}\"></body></html>",
        esc(title),
        ika_diagram::CSS,
        render::CALLOUT_CSS,
        esc(query)
    )
}

pub fn error(status: u16, message: &str) -> String {
    layout(
        &format!("{status}"),
        "",
        &format!(
            "<h1>{status}</h1><p>{}</p><p><a href=\"/\">トップへ</a></p>",
            esc(message)
        ),
    )
}

fn badge(kind: &str, status: &str) -> String {
    if kind.is_empty() {
        return String::new();
    }
    let label = if status.is_empty() || !crate::kb::search::has_status(kind) {
        kind.to_string()
    } else {
        format!("{kind} {status}")
    };
    format!("<span class=\"badge {}\">{}</span>", esc(kind), esc(&label))
}

/// kb/ 配下の Markdown 1件分の見出し情報
pub(super) struct Doc {
    pub rel: PathBuf,
    pub title: String,
    pub kind: String,
    pub date: String,
    pub status: String,
    pub tags: Vec<String>,
}

pub(super) fn docs(root: &Path) -> Result<Vec<Doc>> {
    let mut files = Vec::new();
    check::collect_markdown(&root.join("kb"), &mut files)?;
    files.sort();
    let mut docs = Vec::new();
    for file in files {
        let Ok(text) = fs::read_to_string(&file) else {
            continue;
        };
        let fields = frontmatter::parse(&text).ok().flatten().unwrap_or_default();
        let field = |key: &str| fields.get(key).cloned().unwrap_or_default();
        let rel = file.strip_prefix(root).unwrap_or(&file).to_path_buf();
        let title = Some(field("title"))
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| rel.file_name().unwrap_or_default().to_string_lossy().into());
        docs.push(Doc {
            title,
            kind: field("type"),
            date: field("date"),
            status: field("status"),
            tags: fields
                .get("tags")
                .and_then(|v| frontmatter::list(v).ok())
                .unwrap_or_default(),
            rel,
        });
    }
    Ok(docs)
}

pub(super) fn doc_item(doc: &Doc) -> String {
    format!(
        "<li>{}<a href=\"{}\">{}</a> <span class=\"muted\">{}</span><span class=\"path\">{}</span></li>",
        badge(&doc.kind, &doc.status),
        kb_url(&doc.rel),
        esc(&doc.title),
        esc(&doc.date),
        esc(&doc.rel.to_string_lossy())
    )
}

// ---- トップ ----

pub fn index(root: &Path) -> Result<String> {
    let docs = docs(root)?;
    let mut body = String::from("<h1>ika</h1>");

    body.push_str("<h2>プロダクト</h2><ul class=\"list\">");
    for p in registry::projects(root)? {
        body.push_str(&format!(
            "<li><a href=\"/project/{}\">{}</a> <span class=\"badge\">{}</span> <span class=\"muted\">{}</span></li>",
            esc(&p.name),
            esc(&p.name),
            esc(&p.status),
            esc(&p.summary)
        ));
    }
    body.push_str("</ul><h2>領域</h2><ul class=\"list\">");
    for d in registry::domains(root)? {
        let count = |kind_dir: &str| {
            docs.iter()
                .filter(|doc| {
                    let mut parts = doc.rel.iter();
                    parts.next();
                    parts.next().is_some_and(|p| p.to_string_lossy() == d.name)
                        && parts
                            .next()
                            .is_some_and(|p| p.to_string_lossy() == kind_dir)
                })
                .count()
        };
        body.push_str(&format!(
            "<li><a href=\"/kb/{n}/\">{n}</a> <span class=\"muted\">{} · 決定 {} · 知見 {} · ノート {}</span></li>",
            esc(&d.summary),
            count("decisions"),
            count("insights"),
            count("notes"),
            n = esc(&d.name),
        ));
    }
    body.push_str("</ul>");
    body.push_str(&super::datasets::section(root)?);
    body.push_str(&super::tags::section(&docs));
    body.push_str("<h2>最近の更新</h2><ul class=\"list\">");
    let mut recent: Vec<_> = docs.iter().collect();
    recent.sort_by(|a, b| {
        (std::cmp::Reverse(&a.date), &a.rel).cmp(&(std::cmp::Reverse(&b.date), &b.rel))
    });
    for doc in recent.into_iter().take(15) {
        body.push_str(&doc_item(doc));
    }
    body.push_str("</ul>");
    Ok(layout("ika", "", &body))
}

// ---- ディレクトリ ----

fn crumbs(rel: &Path) -> String {
    let mut out = String::from("<div class=\"crumbs\"><a href=\"/\">ika</a>");
    let mut path = PathBuf::new();
    let parts: Vec<_> = rel.iter().collect();
    for (i, part) in parts.iter().enumerate() {
        path.push(part);
        let name = part.to_string_lossy();
        if i + 1 == parts.len() {
            out.push_str(&format!(" / {}", esc(&name)));
        } else {
            out.push_str(&format!(
                " / <a href=\"{}/\">{}</a>",
                kb_url(&path),
                esc(&name)
            ));
        }
    }
    out.push_str("</div>");
    out
}

pub fn directory(root: &Path, rel: &Path) -> Result<String> {
    let abs = root.join(rel);
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    for entry in fs::read_dir(&abs)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if kind.is_symlink() || name.starts_with('.') {
            continue;
        }
        if kind.is_dir() {
            dirs.push(name);
        } else if name.ends_with(".md") {
            files.push(name);
        }
    }
    dirs.sort();
    files.sort();
    // 日付つきのファイル名（YYYY-MM-DD-…）は新しい順
    if files.iter().all(|f| {
        f.as_bytes().get(4) == Some(&b'-') && f.as_bytes()[..4].iter().all(u8::is_ascii_digit)
    }) {
        files.reverse();
    }

    let mut body = format!(
        "{}<h1>{}/</h1><ul class=\"list\">",
        crumbs(rel),
        esc(&rel.to_string_lossy())
    );
    for name in dirs {
        body.push_str(&format!(
            "<li><a href=\"{}/\">{}/</a></li>",
            kb_url(&rel.join(&name)),
            esc(&name)
        ));
    }
    for name in files {
        let file_rel = rel.join(&name);
        let text = fs::read_to_string(root.join(&file_rel)).unwrap_or_default();
        let fields = frontmatter::parse(&text).ok().flatten().unwrap_or_default();
        let field = |key: &str| fields.get(key).cloned().unwrap_or_default();
        let title = Some(field("title"))
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| name.clone());
        body.push_str(&format!(
            "<li>{}<a href=\"{}\">{}</a> <span class=\"muted\">{}</span><span class=\"path\">{}</span></li>",
            badge(&field("type"), &field("status")),
            kb_url(&file_rel),
            esc(&title),
            esc(&field("date")),
            esc(&name)
        ));
    }
    body.push_str("</ul>");
    Ok(layout(&rel.to_string_lossy(), "", &body))
}

// ---- Markdown ----

/// 本文の相対リンクを、元のファイル（`doc`。ルートからの相対）の場所から解決して `/kb/...` の URL にする。
/// README はプロダクトのページ（`/project/<name>`）にも埋め込むので、ページの URL を基準にすると切れる
fn resolve_link(url: &str, doc: &Path) -> String {
    let split = url.find(['?', '#']).unwrap_or(url.len());
    let (path, suffix) = url.split_at(split);
    let has_scheme = path
        .split('/')
        .next()
        .is_some_and(|first| first.contains(':'));
    if path.is_empty() || path.starts_with('/') || has_scheme {
        return url.to_string();
    }
    let Some(decoded) = percent_decode(path, false) else {
        return url.to_string();
    };
    let mut parts: Vec<String> = doc
        .parent()
        .map(|dir| {
            dir.iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    for segment in decoded.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return url.to_string();
                }
            }
            name => parts.push(name.to_string()),
        }
    }
    let trailing = if decoded.ends_with('/') && !parts.is_empty() {
        "/"
    } else {
        ""
    };
    format!("/{}{trailing}{suffix}", percent_encode(&parts.join("/")))
}

/// Markdown を HTML にする（[`render::markdown`]）。相対リンクは `doc`（元のファイル）を基準に解決する。
/// 図の記法の誤りは、その場所に理由を表示する（取り込んだ資料の図は展開しない）
pub fn markdown(text: &str, doc: &Path) -> String {
    let link = |_kind, url: &str| Href::Url(resolve_link(url, doc));
    render::markdown(
        text,
        &RenderOptions {
            link: &link,
            diagrams: !crate::import::is_source_path(doc),
            id_prefix: "",
            heading_shift: 0,
        },
    )
    .html
}

/// `needles` があれば（検索結果から開いたとき）、見出しと本文の一致をハイライトする
pub fn markdown_page(root: &Path, rel: &Path, text: &str, needles: &[String]) -> Result<String> {
    let fields = frontmatter::parse(text).ok().flatten().unwrap_or_default();
    let field = |key: &str| fields.get(key).cloned().unwrap_or_default();
    let list = |key: &str| {
        fields
            .get(key)
            .and_then(|v| frontmatter::list(v).ok())
            .unwrap_or_default()
    };
    let title = Some(field("title"))
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| rel.file_name().unwrap_or_default().to_string_lossy().into());

    let mut meta = vec![badge(&field("type"), &field("status")), esc(&field("date"))];
    let projects: Vec<_> = list("projects")
        .iter()
        .map(|p| format!("<a href=\"/project/{}\">{}</a>", esc(p), esc(p)))
        .collect();
    if !projects.is_empty() {
        meta.push(format!("プロダクト: {}", projects.join(", ")));
    }
    let tags: Vec<_> = list("tags")
        .iter()
        .map(|t| format!("<a href=\"{}\">#{}</a>", super::tags::tag_url(t), esc(t)))
        .collect();
    if !tags.is_empty() {
        meta.push(tags.join(" "));
    }
    let source = field("source");
    if is_web_url(&source) {
        meta.push(format!("出典: <a href=\"{0}\">{0}</a>", esc(&source)));
    } else if !source.is_empty() {
        meta.push(format!("出典: {}", esc(&source)));
    }

    let article = markdown(frontmatter::body(text), rel);
    let mut body = if needles.is_empty() {
        format!(
            "{}<h1>{}</h1><div class=\"meta\">{}</div><article>{article}</article>",
            crumbs(rel),
            esc(&title),
            meta.join(" · "),
        )
    } else {
        let (article, count) = highlight::html(&article, needles);
        let words: String = needles.iter().map(|n| format!("「{}」", esc(n))).collect();
        let note = if count == 0 {
            format!("{words}は本文に見つかりません（フロントマターで当たった可能性があります）")
        } else {
            format!(
                "{words}を <a href=\"#{}\">{count} か所</a>ハイライト",
                highlight::ANCHOR
            )
        };
        format!(
            "{}<h1>{}</h1><div class=\"meta\">{}</div>\
             <div class=\"meta hl-note\">{note} · <a href=\"{}\">消す</a></div><article>{article}</article>",
            crumbs(rel),
            highlight::text(&title, needles),
            meta.join(" · "),
            kb_url(rel),
        )
    };

    let referrers = kb::inbound(root, rel)?;
    if !referrers.is_empty() {
        body.push_str("<h2>このページへのリンク</h2><ul class=\"list\">");
        let all = docs(root)?;
        for referrer in referrers {
            if let Some(doc) = all.iter().find(|d| d.rel == referrer) {
                body.push_str(&doc_item(doc));
            }
        }
        body.push_str("</ul>");
    }
    Ok(layout(&title, "", &body))
}

// ---- プロダクト ----

pub fn project_page(root: &Path, name: &str) -> Result<Option<String>> {
    let projects = registry::projects(root)?;
    let Some(p): Option<&Project> = projects.iter().find(|p| p.name == name) else {
        return Ok(None);
    };
    let mut meta = vec![format!("<span class=\"badge\">{}</span>", esc(&p.status))];
    if !p.path.is_empty() {
        meta.push(format!("<code>{}</code>", esc(&p.path)));
    }
    if is_web_url(&p.url) {
        meta.push(format!("<a href=\"{0}\">{0}</a>", esc(&p.url)));
    }
    let mut body = format!(
        "<h1>{}</h1><div class=\"muted\">{}</div><div class=\"meta\">{}</div>",
        esc(&p.name),
        esc(&p.summary),
        meta.join(" · ")
    );

    let readme = registry::readme_path(root, name);
    if let Ok(text) = fs::read_to_string(&readme) {
        let rel = readme.strip_prefix(root).unwrap_or(&readme);
        body.push_str(&format!(
            "<h2>概要 <a class=\"muted\" href=\"{}\">README</a></h2><article>{}</article>",
            kb_url(rel),
            markdown(frontmatter::body(&text), rel)
        ));
    }

    let related = project::related(root, name)?;
    body.push_str(&format!(
        "<h2>関連（{} 件）</h2><ul class=\"list\">",
        related.len()
    ));
    for item in &related {
        body.push_str(&format!(
            "<li>{}<a href=\"{}\">{}</a> <span class=\"muted\">{}</span><span class=\"path\">{}</span></li>",
            badge(&item.kind, &item.status),
            kb_url(&item.rel),
            esc(&item.title),
            esc(&item.date),
            esc(&item.rel.to_string_lossy())
        ));
    }
    body.push_str("</ul>");
    Ok(Some(layout(name, "", &body)))
}

// ---- 検索 ----

/// 全文検索で1ページに出す件数
const SEARCH_LIMIT: usize = 50;

fn select(name: &str, label: &str, current: &str, options: &[String]) -> String {
    let mut out = format!(
        "<select name=\"{name}\"><option value=\"\">{}</option>",
        esc(label)
    );
    for option in options {
        let selected = if option == current { " selected" } else { "" };
        out.push_str(&format!("<option{selected}>{}</option>", esc(option)));
    }
    out.push_str("</select>");
    out
}

pub fn search_page(root: &Path, req: &Request) -> Result<String> {
    let param = |key: &str| req.param(key).unwrap_or("").to_string();
    let q = param("q");
    let (domain, project, kind, tag) = (
        param("domain"),
        param("project"),
        param("type"),
        param("tag"),
    );
    let any = req.param("any").is_some();
    let exact = req.param("exact").is_some();

    let domains: Vec<_> = registry::domains(root)?
        .into_iter()
        .map(|d| d.name)
        .collect();
    let projects: Vec<_> = registry::projects(root)?
        .into_iter()
        .map(|p| p.name)
        .collect();
    let kinds: Vec<_> = ["decision", "insight", "project", "spec", "note", "source"]
        .map(String::from)
        .into();

    let mut body = format!(
        "<h1>検索</h1><form class=\"search\" action=\"/search\" method=\"get\">\
         <input name=\"q\" value=\"{}\" placeholder=\"検索語（空白区切り）\" size=\"26\">{}{}{}\
         <input name=\"tag\" value=\"{}\" placeholder=\"タグ\" size=\"10\">\
         <label><input type=\"checkbox\" name=\"any\" value=\"1\"{}> どれか</label>\
         <label><input type=\"checkbox\" name=\"exact\" value=\"1\"{}> 文字列一致</label><button>検索</button></form>",
        esc(&q),
        select("domain", "領域", &domain, &domains),
        select("project", "プロダクト", &project, &projects),
        select("type", "種類", &kind, &kinds),
        esc(&tag),
        if any { " checked" } else { "" },
        if exact { " checked" } else { "" },
    );

    if [&q, &domain, &project, &kind, &tag]
        .iter()
        .all(|v| v.is_empty())
    {
        body.push_str("<p class=\"muted\">検索語か絞り込みを指定してください。日本語の形態素解析つきの全文検索で、文でも引け、関連の強い順に並びます。「文字列一致」にすると、検索語をそのまま含むページを信頼度順（採用済みの決定 → 知見 → プロダクト概要 → 案の決定 → ノート → 覆された決定）に出します。</p>");
        return Ok(layout("検索", "", &body));
    }
    let terms: Vec<String> = q.split_whitespace().map(String::from).collect();
    // ハイライトする語。全文検索は一致行の選び方（fts::needles）にそろえ、形態素の表層形・基本形も含める
    let needles: Vec<String> = if exact {
        terms.iter().map(|t| t.to_lowercase()).collect()
    } else {
        fts::needles(&terms).into_iter().collect()
    };
    // 結果から開くページにも語を渡し、最初の一致まで送る
    let hl_suffix = if needles.is_empty() {
        String::new()
    } else {
        format!(
            "?hl={}#{}",
            percent_encode(&needles.join(" ")),
            highlight::ANCHOR
        )
    };
    let domain = Some(domain).filter(|v| !v.is_empty());
    let project = Some(project).filter(|v| !v.is_empty());
    let tag = Some(tag).filter(|v| !v.is_empty());
    let kind = Some(kind).filter(|v| !v.is_empty());
    let titles = docs(root)?;
    let item = |label: &str, rel: &Path, date: &str, lines: &[(usize, String)]| {
        let title = titles
            .iter()
            .find(|d| d.rel == rel)
            .map(|d| d.title.clone())
            .unwrap_or_else(|| rel.to_string_lossy().into());
        let mut out = format!(
            "<li><span class=\"badge\">{}</span><a href=\"{}{}\">{}</a> <span class=\"muted\">{}</span><span class=\"path\">{}</span>",
            esc(label),
            kb_url(rel),
            esc(&hl_suffix),
            highlight::text(&title, &needles),
            esc(date),
            esc(&rel.to_string_lossy())
        );
        for (n, line) in lines.iter().take(3) {
            out.push_str(&format!(
                "<div class=\"snippet\">{n}: {}</div>",
                highlight::snippet(line, &needles, 200)
            ));
        }
        out.push_str("</li>");
        out
    };

    if exact {
        let args = search::Args {
            terms,
            domain,
            project,
            tag,
            kind,
            status: None,
            any,
            files: false,
        };
        let hits = search::find(root, &args)?;
        body.push_str(&format!(
            "<p class=\"muted\">{} 件（文字列一致・信頼度順）</p><ul class=\"list\">",
            hits.len()
        ));
        for hit in hits {
            body.push_str(&item(&hit.label, &hit.rel, &hit.date, &hit.lines));
        }
    } else {
        let args = fts::Args {
            terms,
            domain,
            project,
            tag,
            kind,
            any,
            limit: Some(SEARCH_LIMIT),
            ..fts::Args::default()
        };
        let found = fts::find(root, &args)?;
        let shown = if found.hits.len() < found.total {
            format!("関連の強い順に上位 {} 件", found.hits.len())
        } else {
            "関連の強い順".to_string()
        };
        let stale = if found.stale {
            "。索引を別の ika が更新中のため、更新前の索引で探しました"
        } else {
            ""
        };
        body.push_str(&format!(
            "<p class=\"muted\">{} 件（{shown}）{stale}</p><ul class=\"list\">",
            found.total
        ));
        for hit in found.hits {
            body.push_str(&item(
                &hit.label,
                Path::new(&hit.rel),
                &hit.date,
                &hit.lines,
            ));
        }
    }
    body.push_str("</ul>");
    Ok(layout("検索", &q, &body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_renders_but_never_emits_raw_html_or_script_links() {
        let html = markdown(
            "# 見出し\n\n<script>alert(1)</script>\n\n[x](javascript:alert(1)) [y](https://example.test/a) [z](../a.md)\n\n| a | b |\n|---|---|\n| 1 | 2 |\n",
            Path::new("kb/tech/notes/x.md"),
        );
        assert!(html.contains("<h1 id=\"見出し\">見出し</h1>"), "{html}");
        assert!(html.contains("<table>"));
        assert!(!html.contains("<script>"), "{html}");
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("javascript:"), "{html}");
        assert!(html.contains("href=\"https://example.test/a\""));
        assert!(html.contains("href=\"/kb/tech/a.md\""), "{html}");
    }

    #[test]
    fn markdown_keeps_single_newlines_as_line_breaks() {
        let html = markdown("1行目\n2行目\n\n3行目", Path::new("kb/x.md"));
        assert!(html.contains("<p>1行目<br />\n2行目</p>"), "{html}");
        assert!(html.contains("<p>3行目</p>"), "{html}");
    }

    #[test]
    fn markdown_links_bare_urls_but_not_inside_code_or_links() {
        let html = markdown(
            "出典 https://example.test/a?x=1&y=2 です。(https://example.test/b). [t](https://example.test/c) `https://example.test/d`\n\n```\nhttps://example.test/e\n```\n\nhttps://example.test/f_g_h\n",
            Path::new("kb/x.md"),
        );
        assert!(
            html.contains("<a href=\"https://example.test/a?x=1&amp;y=2\">"),
            "{html}"
        );
        assert!(
            html.contains("href=\"https://example.test/b\">https://example.test/b</a>)."),
            "{html}"
        );
        assert_eq!(
            html.matches("href=\"https://example.test/c\"").count(),
            1,
            "{html}"
        );
        assert!(!html.contains("href=\"https://example.test/d\""), "{html}");
        assert!(!html.contains("href=\"https://example.test/e\""), "{html}");
        assert!(
            html.contains("href=\"https://example.test/f_g_h\""),
            "{html}"
        );
    }

    #[test]
    fn markdown_resolves_relative_links_from_the_source_file() {
        // README はプロダクトのページにも埋め込むので、ページの URL ではなくファイルの場所が基準
        let html = markdown(
            "[a](../../tech/notes/2026/a.md) [b](specs/b.md#x) [c](./sources/c%20d.md) [d](../other/) [e](#top) [f](/kb/tech/) [g](../../../../up.md) ![i](img/p.png)",
            Path::new("kb/projects/foo/README.md"),
        );
        for href in [
            "/kb/tech/notes/2026/a.md",
            "/kb/projects/foo/specs/b.md#x",
            "/kb/projects/foo/sources/c%20d.md",
            "/kb/projects/other/",
            "#top",
            "/kb/tech/",
            "../../../../up.md",
        ] {
            assert!(html.contains(&format!("href=\"{href}\"")), "{href}: {html}");
        }
        assert!(
            html.contains("src=\"/kb/projects/foo/img/p.png\""),
            "{html}"
        );
    }

    #[test]
    fn esc_escapes_html_metacharacters() {
        assert_eq!(
            esc("<a href=\"x\">&'"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;"
        );
    }

    #[test]
    fn kb_url_encodes_each_segment() {
        assert_eq!(
            kb_url(Path::new("kb/tech/日本 語.md")),
            "/kb/tech/%E6%97%A5%E6%9C%AC%20%E8%AA%9E.md"
        );
    }
}
