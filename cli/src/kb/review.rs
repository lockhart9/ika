//! `ika kb review`: 確かめ直した日を、フロントマターの `reviewed: YYYY-MM-DD` に残す。
//! `stale`・`triage` は `date` と `reviewed` の遅いほうを「最後に確かめた日」として使う。

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use chrono::{Local, NaiveDate};

use super::{atomic, done, guard_symlink, resolve};
use crate::{frontmatter, vcs};

#[derive(clap::Args)]
pub struct Args {
    /// 確かめ直した kb/ のファイル（複数可）。内容を確かめてから付ける
    #[arg(required = true)]
    paths: Vec<String>,
    /// 確かめた日（YYYY-MM-DD。省略時は今日。未来の日付は付けられない）
    #[arg(long)]
    date: Option<String>,
}

/// `YYYY-MM-DD` の日付として読む。形式の誤りと、`today` より後の日付は理由つきで断る
pub fn validate(value: &str, today: NaiveDate) -> Result<NaiveDate, String> {
    let parsed = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .ok()
        // `2026-1-5` のような桁の足りない書き方は受け付けない
        .filter(|d| d.format("%Y-%m-%d").to_string() == value)
        .ok_or_else(|| format!("YYYY-MM-DD で書いてください: {value}"))?;
    if parsed > today {
        return Err(format!("未来の日付にできません: {value}"));
    }
    Ok(parsed)
}

/// 判定に使ってよい確認日。形式の誤りと未来の日付は無いものとして扱う（`ika check` が指摘する）
pub fn usable(reviewed: &str, today: NaiveDate) -> String {
    match validate(reviewed, today) {
        Ok(_) => reviewed.to_string(),
        Err(_) => String::new(),
    }
}

/// 最後に確かめた日: `date` と確認日の遅いほう（どちらも `YYYY-MM-DD` なので文字列で比べられる）
pub fn confirmed(date: &str, reviewed: &str) -> String {
    if reviewed > date { reviewed } else { date }.to_string()
}

/// 確認日が `date` より後のときだけ、表示用の `確認 <日付>` を返す
pub fn label(date: &str, reviewed: &str) -> Option<String> {
    (!reviewed.is_empty() && reviewed > date).then(|| format!("確認 {reviewed}"))
}

/// フロントマターから、表示用の `確認 <日付>`（確認日が `date` より後のときだけ）
pub fn label_of(fields: &BTreeMap<String, String>) -> Option<String> {
    let reviewed = usable(fields.get("reviewed")?, Local::now().date_naive());
    label(fields.get("date").map_or("", String::as_str), &reviewed)
}

/// フロントマターの `reviewed:` を `date` にする。無ければ `date:` の行の次に足す。本文と他の項目は変えない
fn set_reviewed(text: &str, date: &str) -> Result<String> {
    let Some(fields) = frontmatter::parse(text)? else {
        bail!("フロントマターがありません");
    };
    if fields.get("type").is_some_and(|t| t == "source") {
        bail!(
            "取り込んだ資料（type: source）には付けられません。原本を確かめて ika import で取り込み直してください"
        );
    }
    let mut out = String::with_capacity(text.len() + 24);
    let mut lines = text.split_inclusive('\n');
    let mut replaced = false;
    let mut inserted = false;
    // 開きの `---`
    out.push_str(lines.next().unwrap_or_default());
    for line in lines.by_ref() {
        let content = line.trim_end_matches(['\r', '\n']);
        let eol = &line[content.len()..];
        if content.trim_end() == "---" {
            // 閉じる前に `date:` が見つからなかった（ありえないが、落とさずに末尾へ足す）
            if !replaced && !inserted {
                out.push_str(&format!("reviewed: {date}{}", eol_or_newline(eol)));
            }
            out.push_str(line);
            break;
        }
        match content.split_once(':').map(|(key, _)| key.trim()) {
            Some("reviewed") => {
                out.push_str(&format!("reviewed: {date}{eol}"));
                replaced = true;
            }
            Some("date") if !fields.contains_key("reviewed") => {
                out.push_str(line);
                // 最後の行（改行なし）でも次の行が始まるように改行を補う
                out.push_str(&format!(
                    "{}reviewed: {date}{eol}",
                    if eol.is_empty() { "\n" } else { "" }
                ));
                inserted = true;
            }
            _ => out.push_str(line),
        }
    }
    out.extend(lines);
    Ok(out)
}

fn eol_or_newline(eol: &str) -> &str {
    if eol.is_empty() { "\n" } else { eol }
}

pub fn run(root: &Path, args: Args) -> Result<()> {
    let today = Local::now().date_naive();
    let date = match &args.date {
        Some(value) => validate(value, today)
            .map_err(|msg| anyhow::anyhow!("--date は{msg}"))?
            .format("%Y-%m-%d")
            .to_string(),
        None => today.format("%Y-%m-%d").to_string(),
    };

    // 先に全部確かめて、1つでも付けられなければ何も書かない
    let mut targets = Vec::new();
    for input in &args.paths {
        let rel = resolve(root, input)?;
        guard_symlink(root, &rel)?;
        if targets.contains(&rel) {
            continue;
        }
        if rel.extension().is_none_or(|ext| ext != "md") {
            bail!("Markdown ではありません: {}", rel.display());
        }
        let text = std::fs::read_to_string(root.join(&rel))
            .with_context(|| format!("読めません: {}", rel.display()))?;
        set_reviewed(&text, &date).with_context(|| rel.display().to_string())?;
        targets.push(rel);
    }

    for rel in &targets {
        let text = atomic::modify(&root.join(rel), |current| set_reviewed(current, &date))?;
        done(root, &format!("確認日を {date} にしました"), rel, &text)?;
    }
    let names: Vec<_> = targets.iter().map(|p| p.display().to_string()).collect();
    vcs::commit(root, &format!("kb review {}", names.join(" ")));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTE: &str =
        "---\ntitle: t\ntype: note\ndate: 2026-10-01\nprojects: []\ntags: []\n---\n\n本文\n";

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 28).unwrap()
    }

    #[test]
    fn reviewed_goes_after_date_and_is_replaced_in_place() {
        let once = set_reviewed(NOTE, "2026-10-20").unwrap();
        assert_eq!(
            once,
            "---\ntitle: t\ntype: note\ndate: 2026-10-01\nreviewed: 2026-10-20\nprojects: []\ntags: []\n---\n\n本文\n"
        );
        let twice = set_reviewed(&once, "2026-10-25").unwrap();
        assert_eq!(twice.matches("reviewed:").count(), 1);
        assert!(twice.contains("reviewed: 2026-10-25\nprojects"));
        assert!(twice.ends_with("---\n\n本文\n"));
    }

    #[test]
    fn crlf_line_endings_are_kept() {
        let crlf = NOTE.replace('\n', "\r\n");
        let out = set_reviewed(&crlf, "2026-10-20").unwrap();
        assert!(out.contains("date: 2026-10-01\r\nreviewed: 2026-10-20\r\nprojects"));
        assert!(!out.contains("\r\r"));
    }

    #[test]
    fn sources_and_files_without_frontmatter_are_refused() {
        let source = NOTE.replace("type: note", "type: source");
        assert!(set_reviewed(&source, "2026-10-20").is_err());
        assert!(set_reviewed("本文だけ\n", "2026-10-20").is_err());
    }

    #[test]
    fn dates_are_strict_and_not_in_the_future() {
        assert!(validate("2026-10-28", today()).is_ok());
        assert!(
            validate("2026-10-29", today())
                .unwrap_err()
                .contains("未来")
        );
        for bad in ["来週", "", "2026-1-5", "2026/10/20", "2026-13-01"] {
            assert!(
                validate(bad, today()).unwrap_err().contains("YYYY-MM-DD"),
                "{bad}"
            );
        }
    }

    #[test]
    fn confirmed_is_the_later_of_date_and_reviewed() {
        assert_eq!(confirmed("2026-10-01", "2026-10-20"), "2026-10-20");
        assert_eq!(confirmed("2026-10-20", "2026-10-01"), "2026-10-20");
        assert_eq!(confirmed("2026-10-01", ""), "2026-10-01");
        assert_eq!(
            label("2026-10-01", "2026-10-20").as_deref(),
            Some("確認 2026-10-20")
        );
        assert_eq!(label("2026-10-20", "2026-10-01"), None);
        assert_eq!(usable("2999-01-01", today()), "");
        assert_eq!(usable("2026-10-20", today()), "2026-10-20");
    }
}
