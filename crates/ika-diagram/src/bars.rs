//! `bars`: 数値の棒。1行が1本で `名前: 値 | 補足`。値は先頭の数（`,` 区切り・小数可）と、
//! その後ろの単位（`円`・`万円`・`%` など）。棒の長さは最大値に対する比で、単位は全行でそろえる。
//! `*` は主役の棒。
//!
//! ```text
//! スタート: 7,788円
//! シンプル: 12,936円
//! *ビジネス: 26,136円 | いちばん売れる（仮定）
//! ```

use crate::{Line, ParseError, empty, esc, split_label, split_note};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Bars {
    pub bars: Vec<Bar>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bar {
    pub name: String,
    /// 書いたとおりの値（`7,788円`）
    pub label: String,
    pub value: f64,
    pub unit: String,
    pub note: String,
    /// `*` を付けた主役の棒
    pub key: bool,
}

pub(crate) fn parse(lines: &[Line]) -> Result<Bars, Vec<ParseError>> {
    let mut bars = Bars::default();
    let mut errors = Vec::new();
    for line in lines {
        match bar(line.text) {
            Ok(bar) => match bars.bars.first() {
                Some(first) if first.unit != bar.unit => errors.push(ParseError::new(
                    line.no,
                    format!(
                        "単位をそろえます（最初の行は「{}」、この行は「{}」）",
                        first.unit, bar.unit
                    ),
                )),
                _ => bars.bars.push(bar),
            },
            Err(message) => errors.push(ParseError::new(line.no, message)),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    if bars.bars.is_empty() {
        return Err(empty("棒"));
    }
    Ok(bars)
}

fn bar(text: &str) -> Result<Bar, String> {
    let (key, text) = match text.strip_prefix('*') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let (name, rest) = split_label(text).ok_or("「名前: 値」の形で書きます")?;
    if name.is_empty() {
        return Err("棒の名前を書きます".into());
    }
    let (label, note) = split_note(rest);
    let (value, unit) = number(label)?;
    Ok(Bar {
        name: name.to_string(),
        label: label.to_string(),
        value,
        unit: unit.to_string(),
        note: note.to_string(),
        key,
    })
}

/// 先頭の数と、その後ろの単位に分ける
fn number(text: &str) -> Result<(f64, &str), String> {
    if text.starts_with(['-', '−', 'ー', '▲']) {
        return Err(format!("負の値は描けません: {text}"));
    }
    let end = text
        .find(|c: char| !(c.is_ascii_digit() || c == ',' || c == '.'))
        .unwrap_or(text.len());
    let digits = &text[..end];
    if !digits.starts_with(|c: char| c.is_ascii_digit()) {
        return Err(format!("値は数で始めます（例: 7,788円）: {text}"));
    }
    let value = digits
        .replace(',', "")
        .parse::<f64>()
        .map_err(|_| format!("数として読めません: {digits}"))?;
    Ok((value, text[end..].trim()))
}

pub(crate) fn render(bars: &Bars) -> String {
    let max = bars.bars.iter().map(|b| b.value).fold(0.0, f64::max);
    let mut out = String::from("<div class=\"ikd-bar-list\">");
    for bar in &bars.bars {
        let ratio = if max > 0.0 { bar.value / max } else { 0.0 };
        let class = if bar.key {
            "ikd-bar-row ikd-key"
        } else {
            "ikd-bar-row"
        };
        let note = if bar.note.is_empty() {
            String::new()
        } else {
            format!("<small>{}</small>", esc(&bar.note))
        };
        out.push_str(&format!(
            "<div class=\"{class}\"><span class=\"ikd-bar-name\">{}{note}</span>\
             <span class=\"ikd-bar-track\"><span class=\"ikd-bar\" style=\"width:{}%\"></span></span>\
             <span class=\"ikd-bar-val\">{}</span></div>",
            esc(&bar.name),
            percent(ratio),
            esc(&bar.label)
        ));
    }
    out.push_str("</div>");
    out
}

/// 比を百分率の文字にする（小数は 1 桁まで、末尾の 0 は書かない）
fn percent(ratio: f64) -> String {
    let text = format!("{:.1}", ratio * 100.0);
    match text.strip_suffix(".0") {
        Some(whole) => whole.to_string(),
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lines;

    fn parse_src(src: &str) -> Result<Bars, Vec<ParseError>> {
        parse(&lines(src))
    }

    #[test]
    fn reads_values_units_notes_and_the_key_bar() {
        let bars = parse_src("a: 7,788円\n*b：12,936.5円 | 売れ筋\n").unwrap();
        let got: Vec<_> = bars
            .bars
            .iter()
            .map(|b| (b.name.as_str(), b.value, b.unit.as_str(), b.key))
            .collect();
        assert_eq!(
            got,
            [("a", 7788.0, "円", false), ("b", 12936.5, "円", true)]
        );
        assert_eq!(bars.bars[1].label, "12,936.5円");
        assert_eq!(bars.bars[1].note, "売れ筋");

        let man = parse_src("a: 1.5万円\nb: 3 万円").unwrap();
        assert_eq!(
            (man.bars[0].value, man.bars[0].unit.as_str()),
            (1.5, "万円")
        );
        assert_eq!(man.bars[1].unit, "万円");
        let pct = parse_src("a: 30%\nb: 0%").unwrap();
        assert_eq!((pct.bars[0].value, pct.bars[0].unit.as_str()), (30.0, "%"));
        let bare = parse_src("a: 3\nb: 1,200").unwrap();
        assert_eq!(bare.bars[1].value, 1200.0);
    }

    #[test]
    fn rejects_non_numbers_negatives_mixed_units_and_missing_colons() {
        let errors =
            parse_src("a: 約100円\nb: -3円\nc: 100円\nd: 1万円\ne 100円\n: 1円\nf: 1.2.3円")
                .unwrap_err();
        let lines: Vec<_> = errors.iter().map(|e| e.line).collect();
        assert_eq!(lines, [1, 2, 4, 5, 6, 7], "{errors:?}");
        assert!(errors[2].message.contains("単位"), "{errors:?}");
    }

    #[test]
    fn scales_bars_to_the_largest_value_and_keeps_written_values() {
        let html = render(&parse_src("a: 7,788円\n*b: 15,576円 | 注\nc: 0円").unwrap());
        assert!(html.contains(
            "<div class=\"ikd-bar-row\"><span class=\"ikd-bar-name\">a</span>\
             <span class=\"ikd-bar-track\"><span class=\"ikd-bar\" style=\"width:50%\"></span></span>\
             <span class=\"ikd-bar-val\">7,788円</span></div>"
        ));
        assert!(html.contains(
            "<div class=\"ikd-bar-row ikd-key\"><span class=\"ikd-bar-name\">b<small>注</small></span>\
             <span class=\"ikd-bar-track\"><span class=\"ikd-bar\" style=\"width:100%\"></span></span>"
        ));
        assert!(html.contains("style=\"width:0%\""));
        assert_eq!(percent(1.0 / 3.0), "33.3");
    }

    #[test]
    fn all_zero_values_draw_empty_bars() {
        let html = render(&parse_src("a: 0\nb: 0").unwrap());
        assert_eq!(html.matches("style=\"width:0%\"").count(), 2);
    }
}
