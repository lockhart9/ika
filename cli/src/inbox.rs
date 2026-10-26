//! 受信箱: 領域・プロダクト・タグを決めずに即保存する入口（`ika capture`）と、読む・消す（`ika inbox`）。
//! `IKA_HOME/inbox/YYYY-MM-DD.md` に、1回の保存を `## HH:MM` の見出しで区切って追記する。
//! `kb/` の外に置くので、検索・`check`・`export`・`serve` には現れない（整理してノートにして初めて KB に入る）。

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use chrono::{Local, NaiveDate};
use clap::Subcommand;

use crate::{kb, vcs};

const DIR: &str = "inbox";

#[derive(clap::Args)]
pub struct CaptureArgs {
    /// 保存する文章（複数の語は空白でつなぐ。URL もそのまま保存する）。省略すると標準入力から読む
    text: Vec<String>,
    /// 標準入力から読む（複数行の文章用）
    #[arg(long)]
    stdin: bool,
}

#[derive(Subcommand)]
pub enum Command {
    /// 受信箱の中身を日付・時刻の順に全文で出す
    Ls {
        /// この日（YYYY-MM-DD）だけ
        #[arg(long)]
        date: Option<String>,
    },
    /// 整理し終えた日のファイルを消す（履歴は git に残る）
    Rm {
        /// 日付（YYYY-MM-DD）
        date: String,
    },
}

/// 受信箱の1件
pub struct Entry {
    pub date: String,
    pub time: String,
    pub body: String,
}

pub fn capture(root: &Path, args: CaptureArgs) -> Result<()> {
    let text = if args.stdin || args.text.is_empty() {
        kb::read_stdin()?
    } else {
        args.text.join(" ")
    };
    let body = text.trim();
    if body.is_empty() {
        bail!("保存する文章が空です");
    }
    let now = Local::now();
    let date = now.format("%Y-%m-%d").to_string();
    let path = day_path(root, &date);
    fs::create_dir_all(root.join(DIR))?;
    // O_APPEND で1回の書き込みにするので、並行して保存しても互いの内容を壊さない。
    // 末尾の空行が次の保存との区切りになる
    let entry = format!("## {}\n\n{body}\n\n", now.format("%H:%M"));
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?
        .write_all(entry.as_bytes())?;
    let count = parse(&date, &fs::read_to_string(&path)?).len();
    println!("受信箱に追加しました: {DIR}/{date}.md（今日 {count} 件）");
    vcs::commit(root, &format!("inbox capture {date}"));
    Ok(())
}

pub fn run(root: &Path, command: Command) -> Result<()> {
    match command {
        Command::Ls { date } => {
            if let Some(date) = &date {
                check_date(date)?;
            }
            let entries: Vec<Entry> = entries(root)?
                .into_iter()
                .filter(|e| date.as_ref().is_none_or(|d| *d == e.date))
                .collect();
            if entries.is_empty() {
                println!("受信箱は空です");
            }
            for entry in entries {
                println!("■ {} {}\n{}\n", entry.date, entry.time, entry.body);
            }
        }
        Command::Rm { date } => {
            check_date(&date)?;
            let path = day_path(root, &date);
            if !path.is_file() {
                bail!("受信箱に {date} はありません");
            }
            let count = parse(&date, &fs::read_to_string(&path)?).len();
            fs::remove_file(&path)?;
            println!("消しました: {DIR}/{date}.md（{count} 件）");
            vcs::commit(root, &format!("inbox rm {date}"));
        }
    }
    Ok(())
}

/// 受信箱のすべての項目を、日付・時刻の順に返す。受信箱が無ければ空
pub fn entries(root: &Path) -> Result<Vec<Entry>> {
    let dir = root.join(DIR);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut days: Vec<(String, PathBuf)> = Vec::new();
    for item in fs::read_dir(&dir)? {
        let path = item?.path();
        let date = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(".md"))
            .filter(|d| check_date(d).is_ok())
            .map(str::to_string);
        if let Some(date) = date {
            days.push((date, path));
        }
    }
    days.sort();
    let mut out = Vec::new();
    for (date, path) in days {
        out.extend(parse(&date, &fs::read_to_string(path)?));
    }
    Ok(out)
}

fn day_path(root: &Path, date: &str) -> PathBuf {
    root.join(DIR).join(format!("{date}.md"))
}

/// `YYYY-MM-DD` の形だけ受け付ける（パスに使うので `../` などは通さない）
fn check_date(date: &str) -> Result<()> {
    let ok = NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .is_ok_and(|d| d.format("%Y-%m-%d").to_string() == date);
    if !ok {
        bail!("日付は YYYY-MM-DD の形で指定してください: {date}");
    }
    Ok(())
}

/// `## HH:MM` の行で区切って項目に分ける
fn parse(date: &str, text: &str) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::new();
    for line in text.lines() {
        if let Some(time) = line.strip_prefix("## ").filter(|t| is_time(t)) {
            out.push(Entry {
                date: date.to_string(),
                time: time.to_string(),
                body: String::new(),
            });
        } else if let Some(last) = out.last_mut() {
            last.body.push_str(line);
            last.body.push('\n');
        }
    }
    for entry in &mut out {
        entry.body = entry.body.trim().to_string();
    }
    out
}

fn is_time(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() == 5 && b[2] == b':' && [0, 1, 3, 4].iter().all(|&i| b[i].is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_splits_on_time_headings_and_keeps_multiline_bodies() {
        let text = "## 09:05\n\n一行目\n二行目\n\n## 14:32\n\nhttps://example.test/x\n\n";
        let got = parse("2026-10-27", text);
        assert_eq!(got.len(), 2);
        assert_eq!(
            (got[0].time.as_str(), got[0].body.as_str()),
            ("09:05", "一行目\n二行目")
        );
        assert_eq!(got[1].body, "https://example.test/x");
    }

    #[test]
    fn check_date_rejects_paths_and_loose_forms() {
        assert!(check_date("2026-10-27").is_ok());
        for bad in ["2026-13-40", "../kb/x", "2026-1-5", "", "2026-10-27.md"] {
            assert!(check_date(bad).is_err(), "{bad}");
        }
    }
}
