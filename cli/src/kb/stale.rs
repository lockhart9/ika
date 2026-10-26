//! `ika kb stale`: 古くなっていそうなナレッジを洗い出す。書式は変えず、今あるデータだけで判断する。
//!
//! 1. プロダクトの README の日付より後に、リポジトリ（`path`）へコミットがある
//! 2. insight の日付より新しく、タグ（とプロダクト）が重なるノートを、その insight がまだリンクしていない
//! 3. 変わりやすい情報（価格・競合など）の古いノートを、insight・決定・要件書・README が根拠にしている
//! 4. 手元のファイルから取り込んだ資料の原本が、取り込んだときから変わった・見当たらない

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;
use chrono::{Days, Local, NaiveDate};
use sha2::{Digest, Sha256};

use crate::{check, frontmatter, kb, registry};

/// 変わりやすい情報を表すタグの既定
const VOLATILE: &str = "pricing,price,cost,aws-cost,competitor,価格,競合,料金,コスト";

#[derive(clap::Args)]
pub struct Args {
    /// このプロダクトに関わるものだけ
    #[arg(short, long)]
    pub(super) project: Option<String>,
    /// 変わりやすい情報のノートを「古い」とみなす日数
    #[arg(long, default_value_t = 90)]
    pub(super) days: u64,
    /// 変わりやすい情報を表すタグ（カンマ区切り）
    #[arg(long, value_delimiter = ',', default_value = VOLATILE)]
    pub(super) volatile: Vec<String>,
}

/// `--volatile` の既定（`triage` が同じ既定を使う）
pub const DEFAULT_VOLATILE: &str = VOLATILE;

/// kb/ の Markdown 1件分
struct Doc {
    rel: PathBuf,
    title: String,
    kind: String,
    status: String,
    date: String,
    projects: Vec<String>,
    tags: Vec<String>,
    source: String,
    hash: String,
    /// 本文のリンク先（ルートからの相対パス）
    links: HashSet<PathBuf>,
}

impl Doc {
    fn has_project(&self, name: &str) -> bool {
        self.projects.iter().any(|p| p == name)
    }

    /// 根拠として他のノートを引く、確定寄りの資料（知見・生きている決定・要件書・README）
    fn is_authority(&self) -> bool {
        match self.kind.as_str() {
            "insight" | "project" => true,
            "decision" => matches!(self.status.as_str(), "accepted" | "proposed"),
            "spec" => self.status != "dropped",
            _ => false,
        }
    }
}

fn load(root: &Path) -> Result<Vec<Doc>> {
    let mut files = Vec::new();
    check::collect_markdown(&root.join("kb"), &mut files)?;
    files.sort();
    let mut docs = Vec::new();
    for file in files {
        let rel = file.strip_prefix(root).unwrap_or(&file).to_path_buf();
        let text = fs::read_to_string(&file)?;
        let fields = frontmatter::parse(&text).ok().flatten().unwrap_or_default();
        let field = |key: &str| fields.get(key).cloned().unwrap_or_default();
        let list = |key: &str| {
            fields
                .get(key)
                .and_then(|v| frontmatter::list(v).ok())
                .unwrap_or_default()
        };
        let dir = rel.parent().unwrap_or(Path::new("")).to_path_buf();
        let links = check::link_targets(&text)
            .into_iter()
            .map(|target| kb::normalize(&dir.join(target)))
            .collect();
        docs.push(Doc {
            title: Some(field("title"))
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| rel.display().to_string()),
            kind: field("type"),
            status: field("status"),
            date: field("date"),
            projects: list("projects"),
            tags: list("tags"),
            source: field("source"),
            hash: field("hash"),
            links,
            rel,
        });
    }
    Ok(docs)
}

/// 点検1つ分の結果（見出し・直し方・項目）
pub struct Section {
    /// 点検の種類を表す短い名前（`triage` の項目の頭に付ける）
    pub kind: &'static str,
    pub heading: String,
    pub fix: &'static str,
    pub items: Vec<Item>,
}

/// 古そうなものの1件。`text` は印字用（1行目が要約、続く行が詳細）、`weight` は同じ種類の中での大きさ
pub struct Item {
    pub text: String,
    pub weight: usize,
}

/// 4つの点検を、README・知見・変わりやすい情報・取り込み資料の順で返す
pub fn sections(root: &Path, args: &Args) -> Result<[Section; 4]> {
    let docs = load(root)?;
    let today = Local::now().date_naive();
    Ok([
        readmes(root, &docs, args)?,
        insights(&docs, args),
        volatile(&docs, args, today),
        sources(&docs, args),
    ])
}

pub fn run(root: &Path, args: Args) -> Result<()> {
    let sections = sections(root, &args)?;
    let total: usize = sections.iter().map(|s| s.items.len()).sum();
    if total == 0 {
        println!("古くなっていそうなものは見つかりませんでした");
        return Ok(());
    }
    println!("古くなっていそうなナレッジ {total} 件");
    for section in sections.iter().filter(|s| !s.items.is_empty()) {
        println!(
            "\n■ {}（{} 件）\n  直し方: {}",
            section.heading,
            section.items.len(),
            section.fix
        );
        for item in &section.items {
            println!("{}", item.text);
        }
    }
    Ok(())
}

// ---- 1. README とリポジトリ ----

fn readmes(root: &Path, docs: &[Doc], args: &Args) -> Result<Section> {
    let mut items = Vec::new();
    for project in registry::projects(root)? {
        if project.path.is_empty() || args.project.as_ref().is_some_and(|p| *p != project.name) {
            continue;
        }
        let repo = crate::root::expand_tilde(&project.path);
        let readme = registry::readme_path(root, &project.name);
        let rel = readme.strip_prefix(root).unwrap_or(&readme);
        let Some(doc) = docs.iter().find(|d| d.rel == rel) else {
            continue;
        };
        // リポジトリが手元に無い（別のマシンにある）ものは確かめられない
        if !repo.is_dir() || NaiveDate::parse_from_str(&doc.date, "%Y-%m-%d").is_err() {
            continue;
        }
        // README の日付の翌日 0 時より後のコミット（同じ日のコミットは反映済みとみなす）
        let after = format!("--after={} 23:59:59", doc.date);
        let Some(count) = git(&repo, &["rev-list", "--count", &after, "HEAD"]) else {
            continue;
        };
        let count: usize = count.parse().unwrap_or(0);
        if count == 0 {
            continue;
        }
        let latest = git(&repo, &["log", "-1", "--format=%cs"]).unwrap_or_default();
        items.push(Item {
            text: format!(
                "  {}  README {} · その後のコミット {count} 件（最新 {latest}）\n      {}",
                project.name,
                doc.date,
                doc.rel.display()
            ),
            weight: count,
        });
    }
    Ok(Section {
        kind: "README",
        heading: "README がリポジトリより古い".into(),
        fix: "project-sync スキルで README をリポジトリの今の状態に合わせる",
        items,
    })
}

/// リポジトリを読むだけの git。git が無い・リポジトリでないときは None
fn git(repo: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

// ---- 2. 知見と、その後のノート ----

fn insights(docs: &[Doc], args: &Args) -> Section {
    let mut items = Vec::new();
    for insight in docs.iter().filter(|d| d.kind == "insight") {
        if args
            .project
            .as_ref()
            .is_some_and(|p| !insight.has_project(p))
        {
            continue;
        }
        let newer: Vec<&Doc> = docs
            .iter()
            .filter(|n| n.kind == "note" && n.date > insight.date)
            .filter(|n| !insight.links.contains(&n.rel))
            .filter(|n| n.tags.iter().any(|t| insight.tags.contains(t)))
            .filter(|n| {
                insight.projects.is_empty() || n.projects.iter().any(|p| insight.has_project(p))
            })
            .collect();
        if newer.is_empty() {
            continue;
        }
        items.push(Item {
            text: format!(
                "  {}  {}（{}）· その後のノート {} 件\n      {}",
                insight.date,
                insight.title,
                insight.rel.display(),
                newer.len(),
                newer
                    .iter()
                    .map(|n| format!("{} {}", n.date, n.rel.display()))
                    .collect::<Vec<_>>()
                    .join("\n      ")
            ),
            weight: newer.len(),
        });
    }
    Section {
        kind: "知見",
        heading: "新しいノートを取り込んでいない知見".into(),
        fix: "ノートを読み、知見を言い直すか根拠に足す（capture スキルの references/distill.md）。関係なければそのままでよい",
        items,
    }
}

// ---- 3. 変わりやすい情報の古いノート ----

fn volatile(docs: &[Doc], args: &Args, today: NaiveDate) -> Section {
    let cutoff = today
        .checked_sub_days(Days::new(args.days))
        .unwrap_or(today)
        .format("%Y-%m-%d")
        .to_string();
    let tags: BTreeSet<&str> = args.volatile.iter().map(|t| t.trim()).collect();
    let mut items = Vec::new();
    for note in docs.iter().filter(|d| d.kind == "note") {
        if note.date.is_empty() || note.date >= cutoff {
            continue;
        }
        if args.project.as_ref().is_some_and(|p| !note.has_project(p)) {
            continue;
        }
        let matched: Vec<&str> = note
            .tags
            .iter()
            .map(String::as_str)
            .filter(|t| tags.contains(t))
            .collect();
        if matched.is_empty() {
            continue;
        }
        let users: Vec<&Doc> = docs
            .iter()
            .filter(|d| d.is_authority() && d.links.contains(&note.rel))
            .collect();
        if users.is_empty() {
            continue;
        }
        items.push(Item {
            text: format!(
                "  {}  {}（#{}）\n      {}\n      根拠にしている: {}",
                note.date,
                note.title,
                matched.join(" #"),
                note.rel.display(),
                users
                    .iter()
                    .map(|d| d.rel.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            weight: users.len(),
        });
    }
    Section {
        kind: "根拠",
        heading: format!(
            "{} 日より前の変わりやすい情報（価格・競合など）を根拠にしている",
            args.days
        ),
        fix: "今も正しいか確かめ、新しいノートに書いて根拠を差し替える。変わっていれば、根拠にしている側も直す（対象のタグは --volatile で変えられる）",
        items,
    }
}

// ---- 4. 取り込んだ資料の原本 ----

fn sources(docs: &[Doc], args: &Args) -> Section {
    let mut items = Vec::new();
    // 同じ原本を複数のプロダクトで取り込んでいても1回だけハッシュを取る
    let mut hashes: BTreeMap<PathBuf, Option<String>> = BTreeMap::new();
    for doc in docs.iter().filter(|d| d.kind == "source") {
        if args.project.as_ref().is_some_and(|p| !doc.has_project(p)) {
            continue;
        }
        // URL（Drive・Notion など）は手元で確かめられない
        if doc.source.is_empty() || doc.source.contains("://") || doc.hash.is_empty() {
            continue;
        }
        let original = crate::root::expand_tilde(&doc.source);
        let current = hashes
            .entry(original.clone())
            .or_insert_with(|| {
                fs::read(&original)
                    .ok()
                    .map(|bytes| hex(&Sha256::digest(&bytes)))
            })
            .clone();
        let reason = match current {
            None => "原本が見当たりません",
            Some(hash) if hash != doc.hash => "原本が変わりました",
            Some(_) => continue,
        };
        items.push(Item {
            text: format!(
                "  {}  {reason}: {}\n      {}",
                doc.date,
                doc.source,
                doc.rel.display()
            ),
            weight: 1,
        });
    }
    Section {
        kind: "取り込み資料",
        heading: "原本が変わった・見当たらない取り込み資料".into(),
        fix: "変わったものは `ika import <原本> --project <name>` で取り込み直し、要点を書き起こした README・要件書・ノートも見直す。見当たらないものは原本の場所を確かめる",
        items,
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
