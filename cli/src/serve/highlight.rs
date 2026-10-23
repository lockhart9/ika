//! 検索語のハイライト（`<mark>`）。検索結果の抜粋と、結果から開いたページ（`?hl=`）で使う。
//! 大文字小文字は区別しない。重なる・隣り合う一致は1つの `<mark>` にまとめる。

use std::ops::Range;

use super::page::esc;

/// 結果から開いたページで、最初の一致に付ける id（リンクの `#` でそこまで送る）
pub const ANCHOR: &str = "ika-hl";

/// `?hl=` に渡せる語の数と長さの上限
const MAX_NEEDLES: usize = 20;
const MAX_NEEDLE_CHARS: usize = 100;

/// `?hl=` の値（空白区切り）を、探す語の一覧にする
pub fn parse(param: Option<&str>) -> Vec<String> {
    let mut needles: Vec<String> = param
        .unwrap_or("")
        .split_whitespace()
        .map(str::to_lowercase)
        .filter(|n| n.chars().count() <= MAX_NEEDLE_CHARS)
        .collect();
    needles.sort();
    needles.dedup();
    needles.truncate(MAX_NEEDLES);
    needles
}

/// 単位（文字や実体参照）ごとの小文字の形から、一致した単位の範囲を求める
fn unit_ranges<'a>(units: impl Iterator<Item = &'a str>, needles: &[String]) -> Vec<Range<usize>> {
    let mut lowered = String::new();
    let mut starts = Vec::new();
    for unit in units {
        starts.push(lowered.len());
        lowered.push_str(unit);
    }
    let count = starts.len();
    starts.push(lowered.len());

    let mut ranges = Vec::new();
    for needle in needles.iter().filter(|n| !n.is_empty()) {
        for (at, matched) in lowered.match_indices(needle.as_str()) {
            let end = at + matched.len();
            // 単位の途中で始まる・終わる一致は、その単位ごと含める
            let first = starts.partition_point(|&s| s <= at).saturating_sub(1);
            let last = starts.partition_point(|&s| s < end).min(count);
            if first < last {
                ranges.push(first..last);
            }
        }
    }
    ranges.sort_by_key(|r| (r.start, r.end));
    let mut merged: Vec<Range<usize>> = Vec::new();
    for range in ranges {
        match merged.last_mut() {
            Some(prev) if range.start <= prev.end => prev.end = prev.end.max(range.end),
            _ => merged.push(range),
        }
    }
    merged
}

/// 文字列の中で一致した文字の範囲（文字単位の位置）
fn char_ranges(text: &str, needles: &[String]) -> Vec<Range<usize>> {
    let lowered: Vec<String> = text.chars().map(|c| c.to_lowercase().collect()).collect();
    unit_ranges(lowered.iter().map(String::as_str), needles)
}

/// プレーンテキストをエスケープし、一致を `<mark>` で囲む
pub fn text(text: &str, needles: &[String]) -> String {
    let ranges = char_ranges(text, needles);
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut pos = 0;
    for range in ranges {
        out.push_str(&esc(&chars[pos..range.start].iter().collect::<String>()));
        out.push_str("<mark>");
        out.push_str(&esc(&chars[range.clone()].iter().collect::<String>()));
        out.push_str("</mark>");
        pos = range.end;
    }
    out.push_str(&esc(&chars[pos..].iter().collect::<String>()));
    out
}

/// 検索結果の抜粋。長い行は最初の一致が見えるところから切り出す
pub fn snippet(line: &str, needles: &[String], width: usize) -> String {
    let chars: Vec<char> = line.chars().collect();
    let first = char_ranges(line, needles).first().map_or(0, |r| r.start);
    // 一致の前に少し文脈を残す
    let start = if first + 20 > width {
        first.saturating_sub(40)
    } else {
        0
    };
    let end = (start + width).min(chars.len());
    let body: String = chars[start..end].iter().collect();
    format!(
        "{}{}{}",
        if start > 0 { "…" } else { "" },
        text(&body, needles),
        if end < chars.len() { "…" } else { "" }
    )
}

/// 中身を飾らない要素（中の文字を本文として扱わない）
const RAW_ELEMENTS: [&str; 4] = ["style", "script", "textarea", "title"];

/// 組み上がった HTML のテキスト部分（タグの外）だけで一致を `<mark>` で囲む。
/// 一致はタグをまたがない。最初の `<mark>` に [`ANCHOR`] の id を付ける。戻り値の2つめは一致の数
pub fn html(html: &str, needles: &[String]) -> (String, usize) {
    let mut out = String::with_capacity(html.len());
    let mut count = 0;
    let mut raw: Option<&str> = None;
    let mut rest = html;
    while !rest.is_empty() {
        if rest.starts_with('<') {
            let end = rest.find('>').map_or(rest.len(), |i| i + 1);
            let tag = &rest[..end];
            let name: String = tag
                .trim_start_matches(['<', '/'])
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect::<String>()
                .to_ascii_lowercase();
            if let Some(open) = raw {
                if tag.starts_with("</") && name == open {
                    raw = None;
                }
            } else if !tag.starts_with("</") && !tag.ends_with("/>") {
                raw = RAW_ELEMENTS.iter().copied().find(|e| *e == name);
            }
            out.push_str(tag);
            rest = &rest[end..];
            continue;
        }
        let end = rest.find('<').unwrap_or(rest.len());
        let segment = &rest[..end];
        if raw.is_some() {
            out.push_str(segment);
        } else {
            mark_segment(segment, needles, &mut out, &mut count);
        }
        rest = &rest[end..];
    }
    (out, count)
}

/// エスケープ済みのテキスト1区間。実体参照は1文字として探し、元の表記のまま出す
fn mark_segment(segment: &str, needles: &[String], out: &mut String, count: &mut usize) {
    let mut raws: Vec<&str> = Vec::new();
    let mut lowered: Vec<String> = Vec::new();
    let mut rest = segment;
    while let Some(c) = rest.chars().next() {
        let entity = (c == '&')
            .then(|| rest.find(';').filter(|&i| i <= 10))
            .flatten()
            .map(|i| &rest[..=i]);
        let raw = entity.unwrap_or(&rest[..c.len_utf8()]);
        let unit = match entity {
            // 読めない実体参照は、何にも当たらない文字として扱う
            Some(e) => {
                decode_entity(e).map_or("\u{FFFC}".to_string(), |d| d.to_lowercase().collect())
            }
            None => c.to_lowercase().collect(),
        };
        raws.push(raw);
        lowered.push(unit);
        rest = &rest[raw.len()..];
    }
    let mut pos = 0;
    for range in unit_ranges(lowered.iter().map(String::as_str), needles) {
        out.push_str(&raws[pos..range.start].concat());
        if *count == 0 {
            out.push_str(&format!("<mark id=\"{ANCHOR}\">"));
        } else {
            out.push_str("<mark>");
        }
        out.push_str(&raws[range.clone()].concat());
        out.push_str("</mark>");
        *count += 1;
        pos = range.end;
    }
    out.push_str(&raws[pos..].concat());
}

fn decode_entity(entity: &str) -> Option<char> {
    let name = entity.strip_prefix('&')?.strip_suffix(';')?;
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some('\u{a0}'),
        _ => {
            let num = name.strip_prefix('#')?;
            let code = match num.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => num.parse().ok()?,
            };
            char::from_u32(code)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn text_marks_case_insensitively_and_merges_overlaps() {
        assert_eq!(
            text("Rust の申請書 <b>", &n(&["rust", "申請", "請書"])),
            "<mark>Rust</mark> の<mark>申請書</mark> &lt;b&gt;"
        );
        assert_eq!(text("abc", &n(&[])), "abc");
    }

    #[test]
    fn snippet_starts_near_the_first_match() {
        let line = format!("{}目印{}", "あ".repeat(300), "い".repeat(300));
        let out = snippet(&line, &n(&["目印"]), 100);
        assert!(out.starts_with('…') && out.ends_with('…'), "{out}");
        assert!(out.contains("<mark>目印</mark>"), "{out}");
        assert_eq!(snippet("短い行", &n(&["行"]), 100), "短い<mark>行</mark>");
    }

    #[test]
    fn html_marks_only_text_outside_tags() {
        let (out, count) = html(
            "<p class=\"rust\">Rust &amp; rust<a href=\"/rust\">RUST</a></p><style>.rust{}</style>",
            &n(&["rust"]),
        );
        assert_eq!(count, 3);
        assert_eq!(
            out,
            "<p class=\"rust\"><mark id=\"ika-hl\">Rust</mark> &amp; <mark>rust</mark><a href=\"/rust\"><mark>RUST</mark></a></p><style>.rust{}</style>"
        );
    }

    #[test]
    fn html_matches_through_entities_but_not_inside_them() {
        let (out, count) = html("<p>a&amp;b &amp;amp</p>", &n(&["a&b", "amp"]));
        assert_eq!(count, 2, "{out}");
        assert_eq!(
            out,
            "<p><mark id=\"ika-hl\">a&amp;b</mark> &amp;<mark>amp</mark></p>"
        );
    }

    #[test]
    fn parse_lowercases_dedups_and_limits() {
        assert_eq!(parse(Some("Rust rust  申請")), n(&["rust", "申請"]));
        assert!(parse(None).is_empty());
        let long = "x".repeat(101);
        assert!(parse(Some(&long)).is_empty());
    }
}
