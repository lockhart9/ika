//! `seq`: シーケンス図。参加者は登場した順に左から並べ、1行が1メッセージ（または注記）の行になる。
//!
//! ```text
//! 利用者 => ika: export html を打つ
//! ika -> KB: Markdown を読む
//! KB --> ika: 本文とメタ情報
//! note ika: JS なし
//! ```

use crate::{Line, ParseError, empty, esc, split_label};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Seq {
    pub participants: Vec<String>,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// `from`・`to` は参加者の番号
    Message {
        from: usize,
        to: usize,
        arrow: Arrow,
        text: String,
    },
    /// 参加者 `at` の下に置く注記
    Note { at: usize, text: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrow {
    /// `->` 実線（呼び出し）
    Call,
    /// `-->` 破線（返り値）
    Return,
    /// `=>` 強調（主な経路）
    Emphasis,
}

const ARROWS: [(&str, Arrow); 3] = [
    ("-->", Arrow::Return),
    ("->", Arrow::Call),
    ("=>", Arrow::Emphasis),
];

const SHAPE: &str = "「A -> B: 文」の形で書きます（矢印は ->・-->・=> のどれか）";

pub(crate) fn parse(lines: &[Line]) -> Result<Seq, Vec<ParseError>> {
    let mut seq = Seq::default();
    let mut errors = Vec::new();
    for line in lines {
        match step(line.text, &mut seq.participants) {
            Ok(step) => seq.steps.push(step),
            Err(message) => errors.push(ParseError::new(line.no, message)),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    if seq.steps.is_empty() {
        return Err(empty("メッセージ"));
    }
    Ok(seq)
}

fn step(text: &str, participants: &mut Vec<String>) -> Result<Step, String> {
    if let Some(rest) = text.strip_prefix("note ") {
        let (who, body) = split_label(rest).ok_or("「note 参加者: 文」の形で書きます")?;
        let at = participants
            .iter()
            .position(|p| p == who)
            .ok_or_else(|| format!("note の参加者「{who}」は、まだメッセージに出てきていません"))?;
        return Ok(Step::Note {
            at,
            text: body.to_string(),
        });
    }
    let (head, body) = split_label(text).unwrap_or((text, ""));
    let (at, len, arrow) = find_arrow(head).ok_or(SHAPE)?;
    let from = head[..at].trim();
    let to = head[at + len..].trim();
    if from.is_empty() || to.is_empty() {
        return Err("矢印の両側に参加者を書きます".into());
    }
    // `->>`・`--->`・`<-` などの書き間違い
    if from.ends_with(['-', '=', '<'])
        || to.starts_with(['>', '-', '='])
        || find_arrow(to).is_some()
    {
        return Err(SHAPE.into());
    }
    if from == to {
        return Err(format!(
            "自分宛てのメッセージ（{from} -> {from}）は描けません"
        ));
    }
    let mut index = |name: &str| match participants.iter().position(|p| p == name) {
        Some(i) => i,
        None => {
            participants.push(name.to_string());
            participants.len() - 1
        }
    };
    Ok(Step::Message {
        from: index(from),
        to: index(to),
        arrow,
        text: body.to_string(),
    })
}

/// いちばん左の矢印の位置・長さ・種類
fn find_arrow(text: &str) -> Option<(usize, usize, Arrow)> {
    text.char_indices().find_map(|(i, _)| {
        ARROWS
            .iter()
            .find(|(token, _)| text[i..].starts_with(token))
            .map(|(token, arrow)| (i, token.len(), *arrow))
    })
}

/// 参加者 i（0 始まり）は列 2i+1〜2i+3 を占め、中心は列の線 2i+2。
/// メッセージは2人の中心の線から線まで、注記はその参加者の列に置く
pub(crate) fn render(seq: &Seq) -> String {
    let mut out = format!(
        "<div class=\"ikd-seq-grid\" style=\"--n:{};--rows:{}\">",
        seq.participants.len(),
        seq.steps.len()
    );
    for (i, name) in seq.participants.iter().enumerate() {
        out.push_str(&format!(
            "<div class=\"ikd-p\" style=\"--a:{};--b:{}\">{}</div>",
            2 * i + 1,
            2 * i + 3,
            esc(name)
        ));
    }
    for i in 0..seq.participants.len() {
        out.push_str(&format!(
            "<div class=\"ikd-life\" style=\"--a:{};--b:{}\"></div>",
            2 * i + 1,
            2 * i + 3
        ));
    }
    for (r, step) in seq.steps.iter().enumerate() {
        let row = r + 2;
        match step {
            Step::Message {
                from,
                to,
                arrow,
                text,
            } => {
                let mut class = String::from("ikd-m");
                if from > to {
                    class.push_str(" ikd-l");
                }
                match arrow {
                    Arrow::Call => {}
                    Arrow::Return => class.push_str(" ikd-ret"),
                    Arrow::Emphasis => class.push_str(" ikd-hot"),
                }
                out.push_str(&format!(
                    "<div class=\"{class}\" style=\"--r:{row};--a:{};--b:{}\"><span>{}</span></div>",
                    2 * from.min(to) + 2,
                    2 * from.max(to) + 2,
                    esc(text)
                ));
            }
            Step::Note { at, text } => out.push_str(&format!(
                "<div class=\"ikd-note\" style=\"--r:{row};--a:{};--b:{}\">{}</div>",
                2 * at + 1,
                2 * at + 3,
                esc(text)
            )),
        }
    }
    out.push_str("</div>");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lines;

    fn parse_src(src: &str) -> Result<Seq, Vec<ParseError>> {
        parse(&lines(src))
    }

    #[test]
    fn parses_participants_in_order_and_three_arrows() {
        let seq = parse_src(
            "利用者 => ika: export html を打つ\nika -> KB: 読む\nKB --> ika：本文\nnote ika: JS なし\n",
        )
        .unwrap();
        assert_eq!(seq.participants, ["利用者", "ika", "KB"]);
        assert_eq!(
            seq.steps,
            [
                Step::Message {
                    from: 0,
                    to: 1,
                    arrow: Arrow::Emphasis,
                    text: "export html を打つ".into()
                },
                Step::Message {
                    from: 1,
                    to: 2,
                    arrow: Arrow::Call,
                    text: "読む".into()
                },
                Step::Message {
                    from: 2,
                    to: 1,
                    arrow: Arrow::Return,
                    text: "本文".into()
                },
                Step::Note {
                    at: 1,
                    text: "JS なし".into()
                },
            ]
        );
    }

    #[test]
    fn names_may_contain_hyphens_and_text_may_be_empty() {
        let seq = parse_src("api-gw -> db-1").unwrap();
        assert_eq!(seq.participants, ["api-gw", "db-1"]);
    }

    #[test]
    fn reports_every_bad_line_with_its_number() {
        let errors = parse_src(
            "A -> B: ok\nA ->> B: x\n\nA <- B: x\nA -> A: x\nnote C: x\n -> B: x\nA ---> B\n",
        )
        .unwrap_err();
        let lines: Vec<_> = errors.iter().map(|e| e.line).collect();
        assert_eq!(lines, [2, 4, 5, 6, 7, 8], "{errors:?}");
        assert!(errors[2].message.contains("自分宛て"));
        assert!(
            errors[3]
                .message
                .contains("まだメッセージに出てきていません")
        );
    }

    #[test]
    fn renders_grid_positions_like_the_prototype() {
        let seq = parse_src("利用者 => ika: 打つ\nika --> 利用者: 開く\nnote ika: 注").unwrap();
        let html = render(&seq);
        assert!(html.starts_with("<div class=\"ikd-seq-grid\" style=\"--n:2;--rows:3\">"));
        assert!(html.contains("<div class=\"ikd-p\" style=\"--a:1;--b:3\">利用者</div>"));
        assert!(html.contains("<div class=\"ikd-p\" style=\"--a:3;--b:5\">ika</div>"));
        assert!(html.contains("<div class=\"ikd-life\" style=\"--a:3;--b:5\"></div>"));
        assert!(html.contains(
            "<div class=\"ikd-m ikd-hot\" style=\"--r:2;--a:2;--b:4\"><span>打つ</span></div>"
        ));
        assert!(html.contains(
            "<div class=\"ikd-m ikd-l ikd-ret\" style=\"--r:3;--a:2;--b:4\"><span>開く</span></div>"
        ));
        assert!(html.contains("<div class=\"ikd-note\" style=\"--r:4;--a:3;--b:5\">注</div>"));
    }

    #[test]
    fn escapes_text() {
        let html = render(&parse_src("<a> -> b: <script>").unwrap());
        assert!(
            !html.contains("<script>") && !html.contains("<a>"),
            "{html}"
        );
    }
}
