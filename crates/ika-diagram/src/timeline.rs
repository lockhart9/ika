//! `timeline`: 年表。各行は `日付: 文` で、日付は `YYYY-MM-DD` か `YYYY-MM` だけ（絶対日付の決まりを守らせる）。
//! 先頭の `?` は未定の項目で、日付の代わりに自由な文を書ける。書いた順に並べる。
//!
//! ```text
//! 2026-10-21: Mermaid の描画方式を調査
//! ? 未定: 図の記法の細部
//! ```

use crate::{Line, ParseError, empty, esc, split_label};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Timeline {
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// 日付（未定の項目ではそのラベル）
    pub when: String,
    pub text: String,
    pub open: bool,
}

pub(crate) fn parse(lines: &[Line]) -> Result<Timeline, Vec<ParseError>> {
    let mut timeline = Timeline::default();
    let mut errors = Vec::new();
    for line in lines {
        match item(line.text) {
            Ok(item) => timeline.items.push(item),
            Err(message) => errors.push(ParseError::new(line.no, message)),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    if timeline.items.is_empty() {
        return Err(empty("項目"));
    }
    Ok(timeline)
}

fn item(text: &str) -> Result<Item, String> {
    if let Some(rest) = text.strip_prefix('?') {
        let (when, body) = split_label(rest).unwrap_or(("未定", rest.trim()));
        if body.is_empty() {
            return Err("未定の項目の文を書きます（「? 未定: 文」）".into());
        }
        return Ok(Item {
            when: if when.is_empty() { "未定" } else { when }.to_string(),
            text: body.to_string(),
            open: true,
        });
    }
    let (when, body) = split_label(text).ok_or("「YYYY-MM-DD: 文」の形で書きます")?;
    if !is_date(when) {
        return Err(format!(
            "日付は YYYY-MM-DD か YYYY-MM で書きます（未定なら先頭に ?）: {when}"
        ));
    }
    if body.is_empty() {
        return Err("日付のあとに文を書きます".into());
    }
    Ok(Item {
        when: when.to_string(),
        text: body.to_string(),
        open: false,
    })
}

fn is_date(text: &str) -> bool {
    let parts: Vec<_> = text.split('-').collect();
    let number = |s: &str, len: usize| {
        (s.len() == len && s.bytes().all(|b| b.is_ascii_digit()))
            .then(|| s.parse::<u32>().ok())
            .flatten()
    };
    let (Some(year), Some(month)) = (
        parts.first().and_then(|y| number(y, 4)),
        parts.get(1).and_then(|m| number(m, 2)),
    ) else {
        return false;
    };
    if !(1..=12).contains(&month) {
        return false;
    }
    match parts.len() {
        2 => true,
        3 => number(parts[2], 2).is_some_and(|day| (1..=days_in_month(year, month)).contains(&day)),
        _ => false,
    }
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        2 if (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400) => {
            29
        }
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

pub(crate) fn render(timeline: &Timeline) -> String {
    let mut out = String::from("<ol class=\"ikd-tl\">");
    for item in &timeline.items {
        let class = if item.open { " class=\"ikd-open\"" } else { "" };
        out.push_str(&format!(
            "<li{class}><time>{}</time>{}</li>",
            esc(&item.when),
            esc(&item.text)
        ));
    }
    out.push_str("</ol>");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lines;

    fn parse_src(src: &str) -> Result<Timeline, Vec<ParseError>> {
        parse(&lines(src))
    }

    #[test]
    fn parses_dates_months_and_open_items() {
        let timeline =
            parse_src("2026-10-21: 調査\n2024-02：月だけ\n? 未定: 細部\n? 残り\n").unwrap();
        let got: Vec<_> = timeline
            .items
            .iter()
            .map(|i| (i.when.as_str(), i.text.as_str(), i.open))
            .collect();
        assert_eq!(
            got,
            [
                ("2026-10-21", "調査", false),
                ("2024-02", "月だけ", false),
                ("未定", "細部", true),
                ("未定", "残り", true),
            ]
        );
    }

    #[test]
    fn rejects_relative_or_invalid_dates() {
        let errors = parse_src(
            "先週: x\n2026-13-01: x\n2026-02-30: x\n2024-02-29: うるう年\n26-10-21: x\n2026-10-21:\n2026/10/21: x\n",
        )
        .unwrap_err();
        let lines: Vec<_> = errors.iter().map(|e| e.line).collect();
        assert_eq!(lines, [1, 2, 3, 5, 6, 7], "{errors:?}");
    }

    #[test]
    fn renders_items_in_written_order() {
        let html = render(&parse_src("2026-10-23: b\n2026-10-21: a\n? 未定: c").unwrap());
        assert_eq!(
            html,
            "<ol class=\"ikd-tl\"><li><time>2026-10-23</time>b</li><li><time>2026-10-21</time>a</li>\
             <li class=\"ikd-open\"><time>未定</time>c</li></ol>"
        );
    }
}
