//! 外部ナレッジの目録（`data/externals.csv`）。KB の外にあるナレッジ（コネクタでつなぐ Drive・SaaS、
//! 手元のフォルダ、Web）の「どこに何があり、いつ見るか」を、使う人ごとに登録する。
//! ika は中身を取りに行かない（資格情報も持たない）。読むのはエージェントが、クライアントのコネクタ・ファイル・Web で行う。

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use clap::Subcommand;
use serde::{Deserialize, Serialize};

use crate::{registry, vcs};

pub const FILE: &str = "data/externals.csv";
/// connector: クライアントのコネクタ（MCP）で読む / folder: 手元のフォルダ / web: 公開されている Web ページ
pub const KINDS: &[&str] = &["connector", "folder", "web"];
/// `projects`・`domains` の区切り
const SEP: char = ';';

/// data/externals.csv の1行
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct External {
    pub name: String,
    pub kind: String,
    pub location: String,
    pub connector: String,
    /// 関係するプロダクト（`;` 区切り。空はすべて）
    pub projects: String,
    /// 関係する領域（`;` 区切り。空はすべて）
    pub domains: String,
    /// 何が入っていて、いつ見るか
    pub about: String,
}

impl External {
    pub fn project_names(&self) -> Vec<&str> {
        split(&self.projects)
    }

    pub fn domain_names(&self) -> Vec<&str> {
        split(&self.domains)
    }

    /// そのプロダクトに関係するか（`projects` が空ならすべてに関係する）
    pub fn for_project(&self, name: &str) -> bool {
        let names = self.project_names();
        names.is_empty() || names.contains(&name)
    }

    /// その領域に関係するか（`domains` が空ならすべてに関係する）
    pub fn for_domain(&self, name: &str) -> bool {
        let names = self.domain_names();
        names.is_empty() || names.contains(&name)
    }

    /// 種類ごとの読み方
    fn how_to_read(&self) -> String {
        match self.kind.as_str() {
            "connector" => format!(
                "{} のコネクタ（MCP）で場所を開くか検索する。つながっていなければ、接続を頼む",
                self.connector
            ),
            "folder" => {
                "ファイルを直接読む・探す（手元のファイルを読めるクライアントだけ）".to_string()
            }
            "web" => "Web で読む".to_string(),
            _ => String::new(),
        }
    }
}

fn split(list: &str) -> Vec<&str> {
    list.split(SEP)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect()
}

/// 目録を読む。ファイルが無ければ空
pub fn load(root: &Path) -> Result<Vec<External>> {
    if !root.join(FILE).is_file() {
        return Ok(Vec::new());
    }
    registry::load(root, FILE)
}

/// 1行の検証（重複は呼ぶ側で見る）。`projects`・`domains` は登録済みの名前
pub fn validate(
    external: &External,
    projects: &HashSet<&str>,
    domains: &HashSet<&str>,
) -> Vec<String> {
    let mut problems = Vec::new();
    if !registry::is_valid_name(&external.name) {
        problems.push("名前は英小文字・数字・ハイフンにしてください".to_string());
    }
    if !KINDS.contains(&external.kind.as_str()) {
        problems.push(format!(
            "kind は {} のどれかにしてください: {}",
            KINDS.join(" / "),
            external.kind
        ));
    }
    if external.location.trim().is_empty() {
        problems.push("location（場所）が空です".to_string());
    }
    if external.kind == "connector" && external.connector.trim().is_empty() {
        problems.push(
            "kind が connector のときは、どのコネクタで読むか（connector）が要ります".to_string(),
        );
    }
    for name in external.project_names() {
        if !projects.contains(name) {
            problems.push(format!("未登録のプロダクトです: {name}"));
        }
    }
    for name in external.domain_names() {
        if !domains.contains(name) {
            problems.push(format!("未登録の領域です: {name}"));
        }
    }
    if external.about.trim().is_empty() {
        problems.push("about（何が入っていて、いつ見るか）が空です".to_string());
    }
    problems
}

/// `ika check` 用。問題（失敗にする）と注意（失敗にしない）を返す
pub fn check(
    root: &Path,
    projects: &HashSet<&str>,
    domains: &HashSet<&str>,
) -> Result<(Vec<String>, Vec<String>)> {
    let mut problems = Vec::new();
    let mut notices = Vec::new();
    let mut names = HashSet::new();
    for external in load(root)? {
        let mut report = |msg: String| problems.push(format!("{FILE}: {}: {msg}", external.name));
        if !names.insert(external.name.clone()) {
            report("名前が重複しています".to_string());
        }
        for msg in validate(&external, projects, domains) {
            report(msg);
        }
        // 別のマシンでは無いことがあるので、失敗にはしない
        if external.kind == "folder" && !expand_home(&external.location).is_dir() {
            notices.push(format!(
                "{FILE}: {}: フォルダがありません: {}",
                external.name, external.location
            ));
        }
    }
    Ok((problems, notices))
}

fn expand_home(path: &str) -> PathBuf {
    match (path.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => PathBuf::from(path),
    }
}

#[derive(Subcommand)]
pub enum Command {
    /// 外部ナレッジを目録に登録する（中身は取り込まない。読むのはエージェントがコネクタ・ファイル・Web で行う）
    Add {
        /// 英小文字・数字・ハイフン（例: work-drive-specs）
        name: String,
        /// connector（クライアントのコネクタ・MCP で読む）/ folder（手元のフォルダ）/ web（公開されている Web ページ）
        #[arg(long)]
        kind: String,
        /// 場所。コネクタならフォルダ・ページの URL、フォルダなら絶対パス（~ 可）、Web なら URL
        #[arg(long)]
        location: String,
        /// どのコネクタで読むか（kind が connector のとき必須。クライアントでの表示名。例: Google Drive）
        #[arg(long, default_value = "")]
        connector: String,
        /// 関係するプロダクト（複数指定可。省略するとすべてに関係）
        #[arg(short = 'p', long = "project")]
        projects: Vec<String>,
        /// 関係する領域（複数指定可。省略するとすべてに関係）
        #[arg(short = 'd', long = "domain")]
        domains: Vec<String>,
        /// 何が入っていて、いつ見るか（エージェントが見るかを決める材料）
        #[arg(long)]
        about: String,
    },
    /// 登録した外部ナレッジを、種類ごとの読み方つきで一覧する
    List {
        /// このプロダクトに関係するもの（プロダクトの指定が無い行も含む）
        #[arg(short = 'p', long = "project")]
        project: Option<String>,
        /// この領域に関係するもの（領域の指定が無い行も含む）
        #[arg(short = 'd', long = "domain")]
        domain: Option<String>,
    },
    /// 目録から外す（外部の中身には触れない）
    Rm { name: String },
}

pub fn run(root: &Path, command: Command) -> Result<()> {
    match command {
        Command::Add {
            name,
            kind,
            location,
            connector,
            projects,
            domains,
            about,
        } => add(
            root,
            External {
                name,
                kind,
                location,
                connector,
                projects: projects.join(&SEP.to_string()),
                domains: domains.join(&SEP.to_string()),
                about,
            },
        ),
        Command::List { project, domain } => list(root, project.as_deref(), domain.as_deref()),
        Command::Rm { name } => rm(root, &name),
    }
}

fn add(root: &Path, external: External) -> Result<()> {
    let projects = registry::projects(root)?;
    let domains = registry::domains(root)?;
    let project_names: HashSet<&str> = projects.iter().map(|p| p.name.as_str()).collect();
    let domain_names: HashSet<&str> = domains.iter().map(|d| d.name.as_str()).collect();
    let problems = validate(&external, &project_names, &domain_names);
    if !problems.is_empty() {
        bail!("登録できません: {}", problems.join("。"));
    }
    let mut rows = load(root)?;
    if rows.iter().any(|e| e.name == external.name) {
        bail!(
            "すでに登録されています: {}（変えるなら `ika external rm {}` してから登録し直す）",
            external.name,
            external.name
        );
    }
    let name = external.name.clone();
    rows.push(external);
    registry::save(root, FILE, &rows)?;
    vcs::commit(root, &format!("external add {name}"));
    println!("登録しました: {name}（{FILE}）");
    Ok(())
}

fn list(root: &Path, project: Option<&str>, domain: Option<&str>) -> Result<()> {
    let rows = load(root)?;
    if rows.is_empty() {
        println!("外部ナレッジの登録はありません");
        println!(
            "  登録: ika external add <name> --kind connector --location <URL> --connector <コネクタ名> --about <何が入っていて、いつ見るか>"
        );
        return Ok(());
    }
    let selected: Vec<_> = rows
        .iter()
        .filter(|e| project.is_none_or(|p| e.for_project(p)))
        .filter(|e| domain.is_none_or(|d| e.for_domain(d)))
        .collect();
    if selected.is_empty() {
        println!("条件に合う外部ナレッジはありません（全 {} 件）", rows.len());
        return Ok(());
    }
    for (i, e) in selected.iter().enumerate() {
        if i > 0 {
            println!();
        }
        let kind = if e.connector.is_empty() {
            e.kind.clone()
        } else {
            format!("{}: {}", e.kind, e.connector)
        };
        println!("{}  [{kind}]  {}", e.name, e.location);
        println!("  {}", e.about);
        let scope = |names: Vec<&str>| {
            if names.is_empty() {
                "すべて".to_string()
            } else {
                names.join("・")
            }
        };
        println!(
            "  関係: プロダクト {} ／ 領域 {}",
            scope(e.project_names()),
            scope(e.domain_names())
        );
        println!("  読み方: {}", e.how_to_read());
    }
    Ok(())
}

fn rm(root: &Path, name: &str) -> Result<()> {
    let mut rows = load(root)?;
    let before = rows.len();
    rows.retain(|e| e.name != name);
    if rows.len() == before {
        bail!("登録されていません: {name}（`ika external list` で確認してください）");
    }
    registry::save(root, FILE, &rows)?;
    vcs::commit(root, &format!("external rm {name}"));
    println!("目録から外しました: {name}");
    Ok(())
}

/// 書き出し（claude.ai・ChatGPT）に含める行。手元のフォルダはクラウドから読めず、パスも出さないので除く
pub fn for_export(root: &Path, domains: &[String], with_projects: bool) -> Result<Vec<External>> {
    Ok(load(root)?
        .into_iter()
        .filter(|e| e.kind != "folder")
        .filter(|e| domains.is_empty() || domains.iter().any(|d| e.for_domain(d)))
        .filter(|e| with_projects || e.project_names().is_empty())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(kind: &str, connector: &str, projects: &str, domains: &str) -> External {
        External {
            name: "work-drive".into(),
            kind: kind.into(),
            location: "https://drive.google.com/drive/folders/x".into(),
            connector: connector.into(),
            projects: projects.into(),
            domains: domains.into(),
            about: "仕様書".into(),
        }
    }

    #[test]
    fn empty_scope_means_everything() {
        let all = row("web", "", "", "");
        assert!(all.for_project("a") && all.for_domain("tech"));
        let some = row("web", "", "a;b", "business");
        assert!(some.for_project("b") && !some.for_project("c"));
        assert!(some.for_domain("business") && !some.for_domain("tech"));
    }

    #[test]
    fn validation_catches_each_rule() {
        let projects: HashSet<&str> = ["a"].into();
        let domains: HashSet<&str> = ["tech"].into();
        assert!(
            validate(
                &row("connector", "Google Drive", "a", "tech"),
                &projects,
                &domains
            )
            .is_empty()
        );
        for bad in [
            row("drive", "", "", ""),
            row("connector", "", "", ""),
            row("web", "", "zzz", ""),
            row("web", "", "", "zzz"),
            External {
                name: "Bad_Name".into(),
                ..row("web", "", "", "")
            },
            External {
                about: " ".into(),
                ..row("web", "", "", "")
            },
            External {
                location: "".into(),
                ..row("web", "", "", "")
            },
        ] {
            assert_eq!(validate(&bad, &projects, &domains).len(), 1, "{bad:?}");
        }
    }
}
