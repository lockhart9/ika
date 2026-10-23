//! `compare`: 案の比較。字下げしない行が案（`名前 | 説明`）、字下げした行が `+ 長所`・`- 短所`・`= 結論`。
//! 先頭の `*` は採用した案（1つだけ）。決まっていない比較では付けない。
//!
//! ```text
//! A mermaid.js を同梱 | 本家と同じ描画
//!   + 全種類の図を描ける
//!   - 見た目が好みに合わない
//!   = 採用しない
//! * C CSS の部品で描く
//!   + JS なし
//!   = 採用
//! ```

use crate::{Line, ParseError, empty, esc, split_note};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Compare {
    pub choices: Vec<Choice>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub name: String,
    pub summary: String,
    /// `*` を付けた（採用した）案
    pub picked: bool,
    /// 書いた順の長所・短所
    pub points: Vec<Point>,
    pub verdict: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Point {
    Pro(String),
    Con(String),
}

pub(crate) fn parse(lines: &[Line]) -> Result<Compare, Vec<ParseError>> {
    let mut compare = Compare::default();
    let mut errors = Vec::new();
    for line in lines {
        let result = if line.indented {
            match compare.choices.last_mut() {
                Some(choice) => point(line.text, choice),
                None => Err("長所・短所の前に案を書きます".into()),
            }
        } else {
            choice(line.text, &compare.choices).map(|c| compare.choices.push(c))
        };
        if let Err(message) = result {
            errors.push(ParseError::new(line.no, message));
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    if compare.choices.is_empty() {
        return Err(empty("案"));
    }
    Ok(compare)
}

fn choice(text: &str, existing: &[Choice]) -> Result<Choice, String> {
    let (picked, text) = match text.strip_prefix('*') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    if picked && existing.iter().any(|c| c.picked) {
        return Err("採用する案（*）は1つだけです".into());
    }
    let (name, summary) = split_note(text);
    if name.is_empty() {
        return Err("案の名前を書きます".into());
    }
    Ok(Choice {
        name: name.to_string(),
        summary: summary.to_string(),
        picked,
        points: Vec::new(),
        verdict: String::new(),
    })
}

fn point(text: &str, choice: &mut Choice) -> Result<(), String> {
    let mut chars = text.chars();
    let mark = chars.next();
    let body = chars.as_str().trim().to_string();
    if !matches!(mark, Some('+' | '-' | '=')) {
        return Err("字下げした行は「+ 長所」「- 短所」「= 結論」のどれかで書きます".into());
    }
    if body.is_empty() {
        return Err("記号のあとに文を書きます".into());
    }
    match mark {
        Some('+') => choice.points.push(Point::Pro(body)),
        Some('-') => choice.points.push(Point::Con(body)),
        _ if !choice.verdict.is_empty() => return Err("結論（=）は1つの案に1行です".into()),
        _ => choice.verdict = body,
    }
    Ok(())
}

pub(crate) fn render(compare: &Compare) -> String {
    let columns: Vec<_> = compare
        .choices
        .iter()
        .map(|c| if c.picked { "1.3fr" } else { "1fr" })
        .collect();
    let mut out = format!(
        "<div class=\"ikd-cmp-grid\" style=\"grid-template-columns:{};min-width:{}rem\">",
        columns.join(" "),
        compare.choices.len() * 13
    );
    for choice in &compare.choices {
        let class = if choice.picked {
            "ikd-opt ikd-pick"
        } else {
            "ikd-opt"
        };
        out.push_str(&format!(
            "<div class=\"{class}\"><h4>{}</h4>",
            esc(&choice.name)
        ));
        if !choice.summary.is_empty() {
            out.push_str(&format!("<p>{}</p>", esc(&choice.summary)));
        }
        if !choice.points.is_empty() {
            out.push_str("<ul>");
            for point in &choice.points {
                let (class, text) = match point {
                    Point::Pro(text) => ("ikd-pro", text),
                    Point::Con(text) => ("ikd-con", text),
                };
                out.push_str(&format!("<li class=\"{class}\">{}</li>", esc(text)));
            }
            out.push_str("</ul>");
        }
        if !choice.verdict.is_empty() {
            out.push_str(&format!(
                "<span class=\"ikd-verdict\">{}</span>",
                esc(&choice.verdict)
            ));
        }
        out.push_str("</div>");
    }
    out.push_str("</div>");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lines;

    fn parse_src(src: &str) -> Result<Compare, Vec<ParseError>> {
        parse(&lines(src))
    }

    #[test]
    fn parses_choices_points_verdicts_and_the_pick() {
        let compare = parse_src(
            "A 同梱 | 本家と同じ\n  + 全部\n  - 好みでない\n  = 採用しない\n* C CSS\n  + JS なし\n",
        )
        .unwrap();
        assert_eq!(compare.choices[0].name, "A 同梱");
        assert_eq!(compare.choices[0].summary, "本家と同じ");
        assert_eq!(
            compare.choices[0].points,
            [Point::Pro("全部".into()), Point::Con("好みでない".into())]
        );
        assert_eq!(compare.choices[0].verdict, "採用しない");
        assert!(!compare.choices[0].picked && compare.choices[1].picked);
    }

    #[test]
    fn rejects_bad_marks_second_picks_and_verdicts() {
        let errors = parse_src("  + 先に点\n*A\n  ・x\n  =\n  = a\n  = b\n*B\n").unwrap_err();
        let lines: Vec<_> = errors.iter().map(|e| e.line).collect();
        assert_eq!(lines, [1, 3, 4, 6, 7], "{errors:?}");
    }

    #[test]
    fn widens_the_picked_choice() {
        let html = render(&parse_src("A\n  - x\n* B\n  + y\n  = 採用").unwrap());
        assert!(html.starts_with(
            "<div class=\"ikd-cmp-grid\" style=\"grid-template-columns:1fr 1.3fr;min-width:26rem\">"
        ));
        assert!(html.contains("<div class=\"ikd-opt ikd-pick\"><h4>B</h4><ul><li class=\"ikd-pro\">y</li></ul><span class=\"ikd-verdict\">採用</span></div>"));
    }
}
