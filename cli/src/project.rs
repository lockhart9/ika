use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use chrono::Local;
use clap::Subcommand;

use crate::check;
use crate::frontmatter;
use crate::kb::cat;
use crate::new::render;
use crate::registry::{self, Project};
use crate::templates;
use crate::vcs;

#[derive(Subcommand)]
pub enum Command {
    /// 登録済みのプロダクトを一覧する
    List,
    /// プロダクトを登録し、kb/projects/<name>/README.md を作る
    Add {
        /// 英小文字・数字・ハイフン
        name: String,
        /// ローカルリポジトリのパス（コードが無い事業・副業の検討なら省略）
        #[arg(long)]
        path: Option<String>,
        /// 公開URL
        #[arg(long, default_value = "")]
        url: String,
        /// 一行説明
        #[arg(long, default_value = "")]
        summary: String,
        /// active / paused / archived
        #[arg(long, default_value = "active")]
        status: String,
    },
    /// プロダクトの登録情報と、それに関連する決定・知見・ノートの一覧を表示する
    Show {
        name: String,
        /// README も表示する。3000 字を超える README は要点（「現状」「課題・次の打ち手」の節と、ほかの節の見出し）だけ
        #[arg(long)]
        readme: bool,
        /// README を全文で表示する（--readme を兼ねる）
        #[arg(long)]
        full: bool,
        /// 関連の一覧に表示する件数（既定は 20。0 で無制限。取り込んだ資料は 0 のときだけ並べる）
        #[arg(short = 'n', long)]
        limit: Option<usize>,
    },
    /// 登録済みプロダクトの項目を書き換える（指定した項目だけ）
    Set {
        name: String,
        #[arg(long)]
        path: Option<String>,
        #[arg(long)]
        url: Option<String>,
        #[arg(long)]
        summary: Option<String>,
        /// active / paused / archived
        #[arg(long)]
        status: Option<String>,
    },
}

pub fn run(root: &Path, command: Command) -> Result<()> {
    match command {
        Command::List => {
            for p in registry::projects(root)? {
                println!("{:<16} {:<9} {}", p.name, p.status, p.summary);
            }
            Ok(())
        }
        Command::Add {
            name,
            path,
            url,
            summary,
            status,
        } => {
            let project = Project {
                path: path.unwrap_or_default(),
                name,
                url,
                status,
                summary,
            };
            add(root, &project)
        }
        Command::Show {
            name,
            readme,
            full,
            limit,
        } => {
            let readme = match (readme, full) {
                (_, true) => Readme::Full,
                (true, false) => Readme::Digest,
                (false, false) => Readme::None,
            };
            show(root, &name, readme, limit)
        }
        Command::Set {
            name,
            path,
            url,
            summary,
            status,
        } => set(root, &name, path, url, summary, status),
    }
}

/// 一覧での並び順（要件 → 決定 → 知見 → ノート → 取り込んだ資料 → その他）
const SHOW_ORDER: &[&str] = &["spec", "decision", "insight", "note", "source"];

/// `projects:` にそのプロダクトを含む要件・ノート・決定・知見（README 自身は除く）
pub struct Related {
    pub kind: String,
    pub date: String,
    pub status: String,
    pub title: String,
    pub rel: PathBuf,
}

/// 種類の順、同じ種類の中では新しい日付が先
pub fn related(root: &Path, name: &str) -> Result<Vec<Related>> {
    let readme = registry::readme_path(root, name);
    let mut files = Vec::new();
    check::collect_markdown(&root.join("kb"), &mut files)?;
    let mut related = Vec::new();
    for file in files.into_iter().filter(|f| *f != readme) {
        let Ok(text) = fs::read_to_string(&file) else {
            continue;
        };
        let Ok(Some(fields)) = frontmatter::parse(&text) else {
            continue;
        };
        let listed = fields
            .get("projects")
            .and_then(|v| frontmatter::list(v).ok())
            .is_some_and(|items| items.iter().any(|i| i == name));
        if !listed {
            continue;
        }
        let field = |key: &str| fields.get(key).cloned().unwrap_or_default();
        related.push(Related {
            kind: field("type"),
            date: field("date"),
            status: field("status"),
            title: field("title"),
            rel: file.strip_prefix(root).unwrap_or(&file).to_path_buf(),
        });
    }
    let rank = |ty: &str| {
        SHOW_ORDER
            .iter()
            .position(|t| *t == ty)
            .unwrap_or(usize::MAX)
    };
    related.sort_by(|a, b| {
        (rank(&a.kind), std::cmp::Reverse(&a.date), &a.rel).cmp(&(
            rank(&b.kind),
            std::cmp::Reverse(&b.date),
            &b.rel,
        ))
    });
    Ok(related)
}

/// 関連の一覧に表示する件数の既定
const RELATED_LIMIT: usize = 20;
/// これより長い README は、`--readme` では要点だけを表示する（字数）
const README_FULL_LIMIT: usize = 3000;
/// 要点として全文で出す README の節。次に何をするかの判断に要るもの
const README_KEEP: &[&str] = &["現状", "課題・次の打ち手"];

/// `project show` で README をどこまで出すか
#[derive(Clone, Copy, PartialEq)]
enum Readme {
    None,
    /// 短ければ全文、長ければ要点
    Digest,
    Full,
}

fn show(root: &Path, name: &str, with_readme: Readme, limit: Option<usize>) -> Result<()> {
    let projects = registry::projects(root)?;
    let Some(project) = projects.iter().find(|p| p.name == name) else {
        bail!("未登録のプロダクトです: {name}（`ika project list` で確認してください）");
    };
    println!(
        "{}  [{}]  {}",
        project.name, project.status, project.summary
    );
    println!("path: {}", project.path);
    if !project.url.is_empty() {
        println!("url:  {}", project.url);
    }
    let readme = registry::readme_path(root, name);
    let readme_rel = readme.strip_prefix(root).unwrap_or(&readme);
    println!("README: {}", readme_rel.display());

    let related = related(root, name)?;
    println!("\n関連（{} 件）", related.len());
    // 取り込んだ資料は原本の写しで数が多いので、全件（-n 0）のとき以外は件数だけにする
    let all = limit == Some(0);
    let (sources, listed): (Vec<&Related>, Vec<&Related>) = related
        .iter()
        .partition(|item| !all && item.kind == "source");
    let limit = match limit {
        Some(0) => listed.len(),
        Some(n) => n,
        None => RELATED_LIMIT,
    };
    for item in listed.iter().take(limit) {
        let status = if item.status.is_empty() {
            String::new()
        } else {
            format!(" {}", item.status)
        };
        println!(
            "  [{}{status}] {}  {}  {}",
            item.kind,
            item.date,
            item.rel.display(),
            item.title
        );
    }
    if listed.len() > limit {
        println!("  …ほか {} 件（-n で増やせる）", listed.len() - limit);
    }
    if !sources.is_empty() {
        println!(
            "  取り込んだ資料 {} 件（一覧は ika kb search --type source -p {name} -l）",
            sources.len()
        );
    }

    // KB の外にある、このプロダクトに関係するナレッジ（中身は ika が持たないので、名前だけ）
    let externals: Vec<_> = crate::external::load(root)?
        .into_iter()
        .filter(|e| e.for_project(name))
        .map(|e| e.name)
        .collect();
    if !externals.is_empty() {
        println!(
            "\n外部ナレッジ {} 件: {}（場所と読み方は ika external list -p {name}）",
            externals.len(),
            externals.join("・")
        );
    }

    if with_readme == Readme::None {
        return Ok(());
    }
    let text = fs::read_to_string(&readme)?;
    let chars = text.chars().count();
    let digest = (with_readme == Readme::Digest && chars > README_FULL_LIMIT)
        .then(|| cat::digest(&text, README_KEEP))
        .flatten();
    match digest {
        Some(digest) => {
            let date = frontmatter::parse(&text)
                .ok()
                .flatten()
                .and_then(|fields| fields.get("date").cloned())
                .unwrap_or_default();
            println!(
                "\n--- {}（要点。全 {chars} 字・date {date}。全文は --full、節は ika kb cat <path> --section \"見出し\"）---",
                readme_rel.display()
            );
            print!("{digest}");
        }
        None => {
            println!("\n--- {} ---", readme_rel.display());
            print!("{text}");
        }
    }
    Ok(())
}

fn set(
    root: &Path,
    name: &str,
    path: Option<String>,
    url: Option<String>,
    summary: Option<String>,
    status: Option<String>,
) -> Result<()> {
    if path.is_none() && url.is_none() && summary.is_none() && status.is_none() {
        bail!("--path / --url / --summary / --status のいずれかを指定してください");
    }
    if let Some(status) = &status
        && !registry::STATUSES.contains(&status.as_str())
    {
        bail!(
            "status は {} のいずれかです",
            registry::STATUSES.join(" / ")
        );
    }
    let mut projects = registry::projects(root)?;
    let Some(project) = projects.iter_mut().find(|p| p.name == name) else {
        bail!("未登録のプロダクトです: {name}（`ika project list` で確認してください）");
    };
    let before = project.clone();
    if let Some(path) = path {
        project.path = path;
    }
    if let Some(url) = url {
        project.url = url;
    }
    if let Some(summary) = summary {
        project.summary = summary;
    }
    if let Some(status) = status {
        project.status = status;
    }
    let after = project.clone();
    registry::save(root, registry::PROJECTS, &projects)?;

    println!("更新しました: {name}");
    for (key, old, new) in [
        ("path", &before.path, &after.path),
        ("url", &before.url, &after.url),
        ("status", &before.status, &after.status),
        ("summary", &before.summary, &after.summary),
    ] {
        if old != new {
            println!("  {key}: {old} -> {new}");
        }
    }
    vcs::commit(root, &format!("project set {name}"));
    Ok(())
}

fn add(root: &Path, project: &Project) -> Result<()> {
    if !registry::is_valid_name(&project.name) {
        bail!(
            "プロダクト名は英小文字・数字・ハイフンにしてください: {}",
            project.name
        );
    }
    if !registry::STATUSES.contains(&project.status.as_str()) {
        bail!(
            "status は {} のいずれかです",
            registry::STATUSES.join(" / ")
        );
    }
    if registry::projects(root)?
        .iter()
        .any(|p| p.name == project.name)
    {
        bail!("すでに登録されています: {}", project.name);
    }

    let readme = registry::readme_path(root, &project.name);
    if !readme.exists() {
        let template = templates::get("project").expect("埋め込み済みのひな形");
        let date = Local::now().format("%Y-%m-%d").to_string();
        let repo = if project.path.is_empty() {
            "なし".to_string()
        } else {
            format!("`{}`", project.path)
        };
        let content = render(
            template,
            &[
                ("name", &project.name),
                ("date", &date),
                ("summary", &project.summary),
                ("path", &repo),
                ("url", &project.url),
            ],
        );
        fs::create_dir_all(readme.parent().expect("kb/projects 配下のパス"))?;
        fs::write(&readme, content)?;
    }
    registry::append(root, registry::PROJECTS, project)?;

    vcs::commit(root, &format!("project add {}", project.name));
    println!("登録しました: {}", project.name);
    println!("  {}", registry::PROJECTS);
    println!(
        "  {}",
        readme.strip_prefix(root).unwrap_or(&readme).display()
    );
    Ok(())
}
