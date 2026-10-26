//! `ika kb cat`: ファイルの全文、または一部（見出し一覧・節・行範囲・フロントマター）を表示する。
//! 長いノートを全部読まずに、必要なところだけ AI に渡すため。

use std::path::Path;

use anyhow::{Result, bail};

use crate::frontmatter;
use crate::kb::{guard_symlink, read, resolve};

#[derive(clap::Args)]
#[command(group(clap::ArgGroup::new("part").args(["outline", "section", "lines", "frontmatter", "full"])))]
pub struct Args {
    /// kb/ から始まるパス
    path: String,
    /// 見出しの一覧を、各節の行範囲・文字数つきで表示する（全文を読む前に構造を知る）
    #[arg(long)]
    outline: bool,
    /// この見出しの節だけ表示する（次の同じ階層以上の見出しまで。小見出しを含む）。
    /// 同名の見出しが複数あるときは `"## 技術"` のように階層を付ける
    #[arg(long, value_name = "見出し")]
    section: Option<String>,
    /// 行範囲だけ表示する（1 始まり・両端を含む）。`40:80`・`40:`（40 行目から末尾）・`:80`・`40`（1 行）
    #[arg(long, value_name = "START:END")]
    lines: Option<String>,
    /// フロントマターだけ表示する
    #[arg(long)]
    frontmatter: bool,
    /// 大きいファイルでも全文を表示する（既定では 8000 字を超えると見出し一覧だけ）
    #[arg(long)]
    full: bool,
}

/// これより長いファイルは、部分の指定がなければ見出し一覧だけを表示する（字数）
const FULL_LIMIT: usize = 8000;

pub fn run(root: &Path, args: Args) -> Result<()> {
    let rel = resolve(root, &args.path)?;
    guard_symlink(root, &rel)?;
    let text = read(root, &rel)?;

    if args.frontmatter {
        let head = frontmatter::head(&text);
        if head.is_empty() {
            bail!("フロントマターがありません: {}", rel.display());
        }
        print!("{head}");
    } else if args.outline {
        print!("{}", outline(&rel.display().to_string(), &text));
    } else if let Some(section) = &args.section {
        print!("{}", section_text(&text, section)?);
    } else if let Some(range) = &args.lines {
        print!("{}", lines_text(&text, range)?);
    } else if !args.full && text.chars().count() > FULL_LIMIT {
        print!("{}", outline(&rel.display().to_string(), &text));
        eprintln!(
            "大きいファイル（{} 字）なので見出し一覧だけ表示しました。全文は --full、節は --section、行範囲は --lines",
            text.chars().count()
        );
    } else {
        print!("{text}");
    }
    Ok(())
}

#[derive(Debug, PartialEq)]
struct Heading {
    /// `#` の数（1〜6）
    level: usize,
    title: String,
    /// 見出しの行（1 始まり）
    line: usize,
    /// 節の最後の行（次の同じ階層以上の見出しの手前。小見出しを含む）
    end: usize,
}

fn split_lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

/// コードブロックの開始・終了行なら、フェンスの文字と長さを返す
fn fence_of(line: &str) -> Option<(char, usize, &str)> {
    let trimmed = line.trim_start();
    let marker = trimmed.chars().next().filter(|c| matches!(c, '`' | '~'))?;
    let run = trimmed.chars().take_while(|&c| c == marker).count();
    let rest = &trimmed[run..];
    // バッククォートの開始行に同じ文字があれば、1 行のインラインコード（```code```）
    (run >= 3 && !(marker == '`' && rest.contains('`'))).then_some((marker, run, rest))
}

/// ATX 形式（`# 見出し`）の見出しを集める。フロントマターとコードブロックの中は見ない
fn headings(text: &str) -> Vec<Heading> {
    let lines = split_lines(text);
    let skip = frontmatter::head(text).lines().count();
    let mut found: Vec<Heading> = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    for (index, raw) in lines.iter().enumerate().skip(skip) {
        let line = raw.trim_end_matches(['\n', '\r']);
        if let Some((marker, run, rest)) = fence_of(line) {
            match fence {
                None => fence = Some((marker, run)),
                Some((open, len)) if open == marker && run >= len && rest.trim().is_empty() => {
                    fence = None;
                }
                Some(_) => {}
            }
            continue;
        }
        if fence.is_some() {
            continue;
        }
        let level = line.chars().take_while(|&c| c == '#').count();
        let rest = &line[level..];
        if !(1..=6).contains(&level) || !(rest.is_empty() || rest.starts_with([' ', '\t'])) {
            continue;
        }
        let title = rest.trim().trim_end_matches('#').trim();
        if title.is_empty() {
            continue;
        }
        found.push(Heading {
            level,
            title: title.to_string(),
            line: index + 1,
            end: lines.len(),
        });
    }
    for i in 0..found.len() {
        if let Some(next) = found[i + 1..].iter().find(|h| h.level <= found[i].level) {
            found[i].end = next.line - 1;
        }
    }
    found
}

/// 行範囲（1 始まり・両端を含む）の文字数。改行は数えない
fn chars_in(lines: &[&str], from: usize, to: usize) -> usize {
    lines[from - 1..to]
        .iter()
        .map(|line| line.trim_end_matches(['\n', '\r']).chars().count())
        .sum()
}

fn outline(name: &str, text: &str) -> String {
    let lines = split_lines(text);
    let heads = headings(text);
    let front = frontmatter::head(text).lines().count();
    let mut rows: Vec<(String, String)> = Vec::new();
    if front > 0 {
        rows.push((format!("L1-{front}"), "フロントマター".to_string()));
    }
    // 最初の見出しより前の本文（プロダクト概要の冒頭の説明など）
    let first = heads.first().map_or(lines.len() + 1, |h| h.line);
    if first > front + 1 && chars_in(&lines, front + 1, first - 1) > 0 {
        let chars = chars_in(&lines, front + 1, first - 1);
        rows.push((
            format!("L{}-{}", front + 1, first - 1),
            format!("（見出しの前）  {chars}字"),
        ));
    }
    for h in &heads {
        rows.push((
            format!("L{}-{}", h.line, h.end),
            format!(
                "{} {}  {}字",
                "#".repeat(h.level),
                h.title,
                chars_in(&lines, h.line, h.end)
            ),
        ));
    }
    let width = rows.iter().map(|(range, _)| range.len()).max().unwrap_or(0);
    let mut out = format!(
        "{name}  全 {} 行・{} 字\n",
        lines.len(),
        if lines.is_empty() {
            0
        } else {
            chars_in(&lines, 1, lines.len())
        }
    );
    for (range, label) in rows {
        out.push_str(&format!("{range:<width$}  {label}\n"));
    }
    out
}

fn with_newline(mut text: String) -> String {
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

fn section_text(text: &str, query: &str) -> Result<String> {
    let query = query.trim();
    let level = query.chars().take_while(|&c| c == '#').count();
    let (level, title) = if level > 0 {
        (Some(level), query[level..].trim())
    } else {
        (None, query)
    };
    let heads = headings(text);
    let matches: Vec<&Heading> = heads
        .iter()
        .filter(|h| h.title == title && level.is_none_or(|l| h.level == l))
        .collect();
    match matches.as_slice() {
        [h] => {
            let lines = split_lines(text);
            Ok(with_newline(lines[h.line - 1..h.end].concat()))
        }
        [] => {
            let list: String = heads
                .iter()
                .map(|h| format!("\n  L{} {} {}", h.line, "#".repeat(h.level), h.title))
                .collect();
            bail!("見出しが見つかりません: {query}（`--outline` で一覧を確認できます）{list}")
        }
        many => {
            let list: String = many
                .iter()
                .map(|h| format!("\n  L{} {} {}", h.line, "#".repeat(h.level), h.title))
                .collect();
            bail!(
                "見出しが {} か所に一致します。`\"## 見出し\"` のように階層を付けるか、`--lines` で範囲を指定してください{list}",
                many.len()
            )
        }
    }
}

/// `START:END`・`START:`・`:END`・`START` を (開始, 終了) にする。省略は None
fn parse_range(range: &str) -> Result<(Option<usize>, Option<usize>)> {
    let number = |part: &str| -> Result<Option<usize>> {
        let part = part.trim();
        if part.is_empty() {
            return Ok(None);
        }
        match part.parse::<usize>() {
            Ok(n) if n >= 1 => Ok(Some(n)),
            _ => bail!("行番号は 1 以上の整数で指定してください: {range}"),
        }
    };
    match range.split_once(':') {
        Some((start, end)) => Ok((number(start)?, number(end)?)),
        None => {
            let line = number(range)?;
            if line.is_none() {
                bail!("--lines が空です。`40:80` のように指定してください");
            }
            Ok((line, line))
        }
    }
}

fn lines_text(text: &str, range: &str) -> Result<String> {
    let (start, end) = parse_range(range)?;
    let lines = split_lines(text);
    let (start, end) = (start.unwrap_or(1), end.unwrap_or(lines.len()));
    if start > end {
        bail!("開始行が終了行より後です: {range}");
    }
    if start > lines.len() {
        bail!("ファイルは {} 行です: {range}", lines.len());
    }
    let end = end.min(lines.len());
    Ok(with_newline(lines[start - 1..end].concat()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "---\ntitle: T\ntype: note\n---\n冒頭の説明\n\n## 要点\n\n- a\n\n### 詳細\n\n本文\n\n```\n## コード内\n```\n\n## 技術\n\n#タグ\n\n## 技術\n";

    #[test]
    fn finds_headings_outside_code_and_frontmatter() {
        let heads = headings(DOC);
        let found: Vec<_> = heads
            .iter()
            .map(|h| (h.level, h.title.as_str(), h.line, h.end))
            .collect();
        assert_eq!(
            found,
            [
                (2, "要点", 7, 18),
                (3, "詳細", 11, 18),
                (2, "技術", 19, 22),
                (2, "技術", 23, 23),
            ]
        );
    }

    #[test]
    fn longer_fences_are_closed_only_by_matching_ones() {
        let text = "# A\n````\n```\n# 中\n````\n# B\n";
        let titles: Vec<_> = headings(text).into_iter().map(|h| h.title).collect();
        assert_eq!(titles, ["A", "B"]);
    }

    #[test]
    fn section_includes_subsections() {
        assert_eq!(
            section_text(DOC, "要点").unwrap(),
            "## 要点\n\n- a\n\n### 詳細\n\n本文\n\n```\n## コード内\n```\n\n"
        );
        assert_eq!(section_text(DOC, "### 詳細").unwrap().lines().count(), 8);
    }

    #[test]
    fn ambiguous_or_missing_sections_fail() {
        let err = section_text(DOC, "技術").unwrap_err().to_string();
        assert!(err.contains("2 か所") && err.contains("L19") && err.contains("L23"));
        let err = section_text(DOC, "無い").unwrap_err().to_string();
        assert!(err.contains("L7 ## 要点"));
        assert!(section_text(DOC, "### 技術").is_err());
        assert!(section_text(DOC, "## 技術").is_err());
    }

    #[test]
    fn parses_ranges() {
        assert_eq!(parse_range("3:5").unwrap(), (Some(3), Some(5)));
        assert_eq!(parse_range("3:").unwrap(), (Some(3), None));
        assert_eq!(parse_range(":5").unwrap(), (None, Some(5)));
        assert_eq!(parse_range("4").unwrap(), (Some(4), Some(4)));
        for bad in ["", "0:3", "a:3", "-1", "1:b"] {
            assert!(parse_range(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn lines_are_clamped_to_the_end() {
        assert_eq!(lines_text("a\nb\nc", "2:99").unwrap(), "b\nc\n");
        assert_eq!(lines_text("a\nb\nc\n", ":2").unwrap(), "a\nb\n");
        assert!(lines_text("a\nb\n", "3").is_err());
        assert!(lines_text("a\nb\n", "2:1").is_err());
    }

    #[test]
    fn outline_lists_ranges_and_sizes() {
        let out = outline("kb/x.md", DOC);
        assert!(out.starts_with("kb/x.md  全 23 行・"), "{out}");
        assert!(out.contains("L1-4    フロントマター"), "{out}");
        assert!(out.contains("L5-6    （見出しの前）  5字"), "{out}");
        assert!(out.contains("L7-18   ## 要点  "), "{out}");
        assert!(out.contains("L11-18  ### 詳細  "), "{out}");
    }
}
