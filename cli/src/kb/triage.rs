//! `ika kb triage`: 未整理のノートと古くなっていそうなナレッジを、優先順位つきの「今週やること」にまとめる。
//! 判定は `orphans`・`stale` と同じで、KB には何も書かない。
//!
//! 並びは、種類の順（受信箱 → 蒸留 → 知見 → README → 根拠 → 取り込み資料）、同じ種類の中では件数の多い順、同点は文字列順。

use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;

use crate::inbox;
use crate::kb::orphans::{self, Note};
use crate::kb::stale::{self, DEFAULT_VOLATILE};

#[derive(clap::Args)]
pub struct Args {
    /// このプロダクトに関わるものだけ
    #[arg(short, long)]
    project: Option<String>,
    /// 番号を付ける項目の数（残りは件数だけ出す）
    #[arg(long, default_value_t = 10)]
    limit: usize,
    /// 未整理のノートがこの件数以上たまったプロダクトを蒸留の項目にする（未満は「様子見」）
    #[arg(long, default_value_t = 3)]
    min: usize,
    /// 変わりやすい情報のノートを「古い」とみなす日数
    #[arg(long, default_value_t = 90)]
    days: u64,
    /// 変わりやすい情報を表すタグ（カンマ区切り）
    #[arg(long, value_delimiter = ',', default_value = DEFAULT_VOLATILE)]
    volatile: Vec<String>,
}

const DISTILL_FIX: &str = "未整理のノートを読み、複数から言えることを insight に書く（capture スキルの references/distill.md）";

/// 一覧の1項目。`rank` が種類の順
struct Todo {
    rank: usize,
    weight: usize,
    kind: &'static str,
    /// 印字する本文（1行目は種類のあとに続く要約、続く行は詳細）
    text: String,
    fix: String,
}

pub fn run(root: &Path, args: Args) -> Result<()> {
    let project = args.project.as_deref();
    let mut todos: Vec<Todo> = Vec::new();

    // 受信箱: プロダクトに紐づかないので、プロダクトを絞ったときは出さない
    if project.is_none() {
        todos.extend(inbox_todo(root)?);
    }

    // 蒸留: 未整理のノートがたまったプロダクト。満たないものは様子見
    let mut watch: Vec<(String, usize)> = Vec::new();
    for (name, notes) in orphans::find(root, project)? {
        if notes.len() >= args.min {
            todos.push(distill(&name, &notes));
        } else {
            watch.push((name, notes.len()));
        }
    }

    // 古そうなナレッジ。stale の4つの点検（README・知見・根拠・取り込み資料）を、種類の順に入れ替える
    let stale_args = stale::Args {
        project: args.project.clone(),
        days: args.days,
        volatile: args.volatile.clone(),
    };
    let sections = stale::sections(root, &stale_args)?;
    // sections の並びは [README, 知見, 根拠, 取り込み資料]。triage では 知見(2) → README(3) → 根拠(4) → 取り込み資料(5)
    for (index, section) in sections.into_iter().enumerate() {
        let rank = [3, 2, 4, 5][index];
        for item in section.items {
            todos.push(Todo {
                rank,
                weight: item.weight,
                kind: section.kind,
                text: item.text.trim_start().to_string(),
                fix: section.fix.to_string(),
            });
        }
    }

    todos.sort_by(|a, b| {
        (a.rank, Reverse(a.weight), &a.text).cmp(&(b.rank, Reverse(b.weight), &b.text))
    });

    if todos.is_empty() {
        println!("今週やることはありません");
    } else {
        let shown = todos.len().min(args.limit);
        if shown < todos.len() {
            println!("今週やること（上位 {shown} 件 / 全 {} 件）", todos.len());
        } else {
            println!("今週やること（{} 件）", todos.len());
        }
        for (n, todo) in todos.iter().take(args.limit).enumerate() {
            let mut lines = todo.text.lines();
            println!(
                "\n{}. 【{}】{}",
                n + 1,
                todo.kind,
                lines.next().unwrap_or("")
            );
            for line in lines {
                println!("{line}");
            }
            println!("   直し方: {}", todo.fix);
        }
        if shown < todos.len() {
            println!(
                "\nほか {} 件。全部は `ika kb orphans`・`ika kb stale` で見られます（`--limit` で増やせます）",
                todos.len() - shown
            );
        }
    }

    if !watch.is_empty() {
        let list: Vec<String> = watch
            .iter()
            .map(|(name, count)| format!("{name} {count} 件"))
            .collect();
        println!(
            "\n様子見（未整理のノートが {} 件未満）: {}",
            args.min,
            list.join("、")
        );
    }
    Ok(())
}

/// 未整理のノートがたまったプロダクト1つ分の蒸留の項目
fn distill(name: &str, notes: &[Note]) -> Todo {
    let mut text = format!("{name}  未整理のノート {} 件", notes.len());
    for note in notes {
        let others: Vec<&str> = note
            .projects
            .iter()
            .map(String::as_str)
            .filter(|p| *p != name)
            .collect();
        let also = if others.is_empty() {
            String::new()
        } else {
            format!("（ほか {} にも関連）", others.join("・"))
        };
        text.push_str(&format!(
            "\n      {}  {}{also}\n          {}",
            note.date,
            note.title,
            note.rel.display()
        ));
    }
    // 2件以上が共有するタグは、まとめ方の候補
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for note in notes {
        for tag in &note.tags {
            *counts.entry(tag.as_str()).or_default() += 1;
        }
    }
    let mut shared: Vec<(&str, usize)> = counts.into_iter().filter(|(_, n)| *n >= 2).collect();
    shared.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    if !shared.is_empty() {
        let tags: Vec<String> = shared
            .iter()
            .take(3)
            .map(|(tag, n)| format!("#{tag}（{n} 件）"))
            .collect();
        text.push_str(&format!("\n      まとめ方の候補: {}", tags.join("、")));
    }
    Todo {
        rank: 1,
        weight: notes.len(),
        kind: "蒸留",
        text,
        fix: DISTILL_FIX.to_string(),
    }
}

const INBOX_FIX: &str = "capture スキルの references/inbox.md で、ノートにする・既存のノートへ追記する・捨てるに振り分け、済んだ日を `ika inbox rm <日付>` で消す";

/// 受信箱に項目があれば、その全体で1つの項目にする。先頭の数件は1行目だけ見せる
fn inbox_todo(root: &Path) -> Result<Option<Todo>> {
    const SHOWN: usize = 5;
    let entries = inbox::entries(root)?;
    let Some(oldest) = entries.first() else {
        return Ok(None);
    };
    let mut days: Vec<&str> = entries.iter().map(|e| e.date.as_str()).collect();
    days.dedup();
    let mut text = format!(
        "受信箱  {} 件（{} 日分、最も古いのは {}）",
        entries.len(),
        days.len(),
        oldest.date
    );
    for entry in entries.iter().take(SHOWN) {
        let first = entry.body.lines().next().unwrap_or("");
        let short: String = first.chars().take(50).collect();
        let ellipsis = if first.chars().count() > 50 {
            "…"
        } else {
            ""
        };
        text.push_str(&format!(
            "\n      {} {}  {short}{ellipsis}",
            entry.date, entry.time
        ));
    }
    if entries.len() > SHOWN {
        text.push_str(&format!("\n      ほか {} 件", entries.len() - SHOWN));
    }
    Ok(Some(Todo {
        rank: 0,
        weight: entries.len(),
        kind: "受信箱",
        text,
        fix: INBOX_FIX.to_string(),
    }))
}
