//! `ika kb search`: フロントマターで絞り込んで本文を検索する（rg が使えない外部作業中向け）

use std::cmp::Reverse;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::check;
use crate::frontmatter;
use crate::kb::review;

#[derive(clap::Args)]
pub struct Args {
    /// 検索語（大文字小文字は区別しない。複数ならすべてを含むファイルだけ）
    pub(crate) terms: Vec<String>,
    /// 領域（kb/<領域>/ に絞る）
    #[arg(short, long)]
    pub(crate) domain: Option<String>,
    /// プロダクト名（フロントマターの projects に含まれるもの）
    #[arg(short, long)]
    pub(crate) project: Option<String>,
    /// タグ（フロントマターの tags に含まれるもの）
    #[arg(short, long)]
    pub(crate) tag: Option<String>,
    /// 種類（project / spec / note / insight / decision / source）
    #[arg(long = "type")]
    pub(crate) kind: Option<String>,
    /// decision の status（proposed / accepted / superseded / rejected）、spec の status（draft / approved / done / dropped）
    #[arg(long)]
    pub(crate) status: Option<String>,
    /// 検索語のどれか1つでも含めば対象にする（表記ゆれ用。既定はすべて含むファイル）
    #[arg(long)]
    pub(crate) any: bool,
    /// ファイル名だけ表示する
    #[arg(short = 'l', long)]
    pub(crate) files: bool,
    /// 表示するファイルの件数（既定は 20。-l か検索語なしなら 50。0 で無制限）
    #[arg(short = 'n', long)]
    pub(crate) limit: Option<usize>,
}

/// 表示するファイルの件数の既定（一致行つき）
const DEFAULT_LIMIT: usize = 20;
/// `-l`（ファイル名だけ）と、検索語なし（一致行が出ない）のときの既定。1 件が短いので多くしてよい
const FILES_LIMIT: usize = 50;
/// 1 ファイルあたりに出す一致行の数
const MAX_LINES: usize = 3;
/// 一致行を切り詰める長さ（字）
const LINE_WIDTH: usize = 160;

/// 一致したファイル1件分
pub struct Hit {
    rank: usize,
    pub date: String,
    /// 確かめ直した日の表示（`確認 2026-10-20`。`date` より後のときだけ）
    pub reviewed: Option<String>,
    pub rel: PathBuf,
    pub label: String,
    pub lines: Vec<(usize, String)>,
}

/// 条件に合うファイルを、信頼度順（確定した判断 → 知見 → … → ノート）・新しい日付順で返す
pub fn find(root: &Path, args: &Args) -> Result<Vec<Hit>> {
    let base = root.join("kb");
    let mut files = Vec::new();
    check::collect_markdown(&base, &mut files)?;

    let terms: Vec<String> = args.terms.iter().map(|t| t.to_lowercase()).collect();
    let mut hits = Vec::new();
    for file in files {
        let rel = file.strip_prefix(root).unwrap_or(&file);
        if args
            .domain
            .as_ref()
            .is_some_and(|d| !rel.starts_with(Path::new("kb").join(d)))
        {
            continue;
        }
        let text = fs::read_to_string(&file)?;
        if !matches_front(&text, args) {
            continue;
        }
        let lower = text.to_lowercase();
        let found = |t: &String| lower.contains(t.as_str());
        if !terms.is_empty()
            && !(if args.any {
                terms.iter().any(found)
            } else {
                terms.iter().all(found)
            })
        {
            continue;
        }
        let lines = text
            .lines()
            .enumerate()
            .filter(|(_, line)| {
                let line = line.to_lowercase();
                terms.iter().any(|t| line.contains(t.as_str()))
            })
            .map(|(n, line)| (n + 1, line.trim().to_string()))
            .collect();
        let (rank, date, label) = weight(&text);
        let reviewed = frontmatter::parse(&text)
            .ok()
            .flatten()
            .and_then(|fields| review::label_of(&fields));
        hits.push(Hit {
            rank,
            date,
            reviewed,
            rel: rel.to_path_buf(),
            label,
            lines,
        });
    }
    // 確定した判断・蒸留した知見を前に、生のメモを後ろに。同じ順位では新しい日付が先
    hits.sort_by(|a, b| {
        (a.rank, Reverse(&a.date), &a.rel).cmp(&(b.rank, Reverse(&b.date), &b.rel))
    });
    Ok(hits)
}

pub fn run(root: &Path, args: Args) -> Result<()> {
    let hits = find(root, &args)?;
    let limit = match args.limit {
        Some(0) => hits.len(),
        Some(n) => n,
        None if args.files || args.terms.is_empty() => FILES_LIMIT,
        None => DEFAULT_LIMIT,
    };
    let terms: Vec<String> = args.terms.iter().map(|t| t.to_lowercase()).collect();
    for hit in hits.iter().take(limit) {
        if args.files {
            println!("{}", hit.rel.display());
            continue;
        }
        let reviewed = hit
            .reviewed
            .as_ref()
            .map_or(String::new(), |r| format!(" {r}"));
        println!(
            "{}  [{}] {}{reviewed}",
            hit.rel.display(),
            hit.label,
            hit.date
        );
        for (n, line) in hit.lines.iter().take(MAX_LINES) {
            println!("  {n}: {}", clip(line, &terms, LINE_WIDTH));
        }
        if hit.lines.len() > MAX_LINES {
            println!("  …ほか {} 行", hit.lines.len() - MAX_LINES);
        }
    }
    if limit < hits.len() {
        eprintln!("{} 件（上位 {limit} 件を表示。-n で増やせる）", hits.len());
    } else {
        eprintln!("{} 件", hits.len());
    }
    Ok(())
}

/// 長い行を `width` 字に切る。最初に当たった語が残るよう、その少し前から取り、切った側に `…` を付ける
fn clip(line: &str, terms: &[String], width: usize) -> String {
    let chars: Vec<char> = line.chars().collect();
    if chars.len() <= width {
        return line.to_string();
    }
    let lower = line.to_lowercase();
    let first = terms
        .iter()
        .filter_map(|t| lower.find(t.as_str()))
        .min()
        .map_or(0, |byte| lower[..byte].chars().count())
        .min(chars.len() - 1);
    // 一致の前に少し文脈を残す
    let start = if first + 20 > width {
        first.saturating_sub(40)
    } else {
        0
    };
    let end = (start + width).min(chars.len());
    format!(
        "{}{}{}",
        if start > 0 { "…" } else { "" },
        chars[start..end].iter().collect::<String>(),
        if end < chars.len() { "…" } else { "" }
    )
}

/// status を表示する種類（decision・spec）
pub fn has_status(kind: &str) -> bool {
    matches!(kind, "decision" | "spec")
}

/// 並び順（小さいほど先）・日付・表示用のラベル。種類ごとの信頼度に応じて重みを付ける:
/// 採用済みの決定 → 知見 → プロダクト概要・承認済みと実装済みの要件 → 案の決定・下書きの要件 → ノート・取り込んだ資料
/// → 覆された・却下の決定・取り下げた要件 → その他
pub(crate) fn weight(text: &str) -> (usize, String, String) {
    let fields = frontmatter::parse(text).ok().flatten().unwrap_or_default();
    let field = |key: &str| fields.get(key).map(String::as_str).unwrap_or("");
    let (kind, status) = (field("type"), field("status"));
    let rank = match (kind, status) {
        ("decision", "accepted") => 0,
        ("insight", _) => 1,
        ("project", _) | ("spec", "approved" | "done") => 2,
        ("decision", "proposed") | ("spec", "draft") => 3,
        ("note" | "source", _) => 4,
        ("decision" | "spec", _) => 5,
        _ => 6,
    };
    let label = if status.is_empty() || !has_status(kind) {
        kind.to_string()
    } else {
        format!("{kind} {status}")
    };
    (rank, field("date").to_string(), label)
}

/// フロントマターの条件（project / tag / type / status）をすべて満たすか
fn matches_front(text: &str, args: &Args) -> bool {
    if args.project.is_none() && args.tag.is_none() && args.kind.is_none() && args.status.is_none()
    {
        return true;
    }
    let Ok(Some(fields)) = frontmatter::parse(text) else {
        return false;
    };
    let has = |key: &str, want: &str| {
        fields
            .get(key)
            .and_then(|v| frontmatter::list(v).ok())
            .is_some_and(|items| items.iter().any(|i| i == want))
    };
    let is = |key: &str, want: &str| fields.get(key).is_some_and(|v| v == want);
    args.project.as_deref().is_none_or(|p| has("projects", p))
        && args.tag.as_deref().is_none_or(|t| has("tags", t))
        && args.kind.as_deref().is_none_or(|k| is("type", k))
        && args.status.as_deref().is_none_or(|s| is("status", s))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        args: Args,
    }

    fn args(argv: &[&str]) -> Args {
        Cli::parse_from(std::iter::once("ika").chain(argv.iter().copied())).args
    }

    const NOTE: &str = "---\ntitle: t\ntype: decision\nprojects: [foo, bar]\ntags: [auth]\nstatus: accepted\n---\nbody\n";

    #[test]
    fn weight_puts_settled_knowledge_before_raw_notes() {
        let doc = |ty: &str, status: &str| {
            format!(
                "---\ntitle: t\ntype: {ty}\ndate: 2026-10-01\nprojects: []\ntags: []\n{status}---\n"
            )
        };
        let rank = |ty: &str, status: &str| weight(&doc(ty, status)).0;
        let accepted = rank("decision", "status: accepted\n");
        let order = [
            accepted,
            rank("insight", ""),
            rank("project", ""),
            rank("decision", "status: proposed\n"),
            rank("note", ""),
            rank("decision", "status: superseded\n"),
            weight("フロントマターなし").0,
        ];
        assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}");
        assert_eq!(
            weight(&doc("decision", "status: accepted\n")).2,
            "decision accepted"
        );
        assert_eq!(weight(&doc("note", "")).2, "note");
    }

    #[test]
    fn weight_places_specs_by_status() {
        let doc = |status: &str| {
            format!("---\ntitle: t\ntype: spec\nprojects: [foo]\ntags: []\nstatus: {status}\n---\n")
        };
        let rank = |status: &str| weight(&doc(status)).0;
        assert_eq!(rank("approved"), rank("done"));
        assert!(rank("done") < rank("draft") && rank("draft") < rank("dropped"));
        assert_eq!(weight(&doc("draft")).2, "spec draft");
    }

    #[test]
    fn clip_keeps_the_first_match_in_view() {
        let terms = vec!["語".to_string()];
        let short = "短い行 語";
        assert_eq!(clip(short, &terms, 160), short);

        let late = format!("{}語{}", "あ".repeat(600), "い".repeat(400));
        let clipped = clip(&late, &terms, 160);
        assert!(clipped.contains('語'), "{clipped}");
        assert!(clipped.starts_with('…') && clipped.ends_with('…'));
        assert!(
            clipped.chars().count() <= 162,
            "{}",
            clipped.chars().count()
        );

        let early = format!("語{}", "い".repeat(400));
        let clipped = clip(&early, &terms, 160);
        assert!(clipped.starts_with('語') && clipped.ends_with('…'));
        assert!(clipped.chars().count() <= 161);
    }

    #[test]
    fn clip_is_case_insensitive_and_char_safe() {
        let terms = vec!["review".to_string()];
        let line = format!("{}Review{}", "日本語".repeat(100), "x".repeat(300));
        let clipped = clip(&line, &terms, 160);
        assert!(clipped.contains("Review"), "{clipped}");
    }

    #[test]
    fn filters_on_frontmatter() {
        assert!(matches_front(NOTE, &args(&["-p", "bar", "-t", "auth"])));
        assert!(matches_front(
            NOTE,
            &args(&["--type", "decision", "--status", "accepted"])
        ));
        assert!(!matches_front(NOTE, &args(&["-p", "baz"])));
        assert!(!matches_front(NOTE, &args(&["--status", "rejected"])));
        assert!(!matches_front("no frontmatter", &args(&["-t", "auth"])));
        assert!(matches_front("no frontmatter", &args(&[])));
    }
}
