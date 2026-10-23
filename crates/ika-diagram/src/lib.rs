//! KB の Markdown に書く図の記法を解析し、CSS だけで描く HTML にする。
//!
//! 図は言語名つきのコードブロック（` ```seq ` など）で書き、1行が1要素。`#` で始まる行はコメント。
//! 依存なし・入出力なし。色とフォントは CSS の変数で受け取る（[`CSS`] の `:root` が既定値で、
//! ページ側で上書きできる）。JavaScript は使わない。

use std::fmt;

mod class;
mod compare;
mod er;
mod flow;
mod layers;
mod seq;
mod timeline;

pub use class::{Class, Classes, Member};
pub use compare::{Choice, Compare, Point};
pub use er::{Attr, Entity, Er, Relation};
pub use flow::{Branch, Flow, Node, NodeKind};
pub use layers::{Layer, Layers, Tone};
pub use seq::{Arrow, Seq, Step};
pub use timeline::{Item, Timeline};

/// 図として扱う言語名
pub const LANGS: &[&str] = &[
    "seq", "flow", "layers", "er", "class", "timeline", "compare",
];

/// 図の部品のスタイル。`:root` に色・フォントの変数の既定値（ライト・ダーク）を持つ
pub const CSS: &str = include_str!("style.css");

/// 解析した図
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Diagram {
    Seq(Seq),
    Flow(Flow),
    Layers(Layers),
    Er(Er),
    Class(Classes),
    Timeline(Timeline),
    Compare(Compare),
}

/// 記法の誤り。`line` はブロックの中の行番号（1 始まり）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl ParseError {
    fn new(line: usize, message: impl Into<String>) -> Self {
        ParseError {
            line,
            message: message.into(),
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}行目: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

/// `lang` の記法で `src` を解析する。`lang` が図の言語名でなければ `None`。
/// 誤りは見つかった分をすべて返す（1つでもあれば図にしない）
pub fn parse(lang: &str, src: &str) -> Option<Result<Diagram, Vec<ParseError>>> {
    let lines = lines(src);
    Some(match lang {
        "seq" => seq::parse(&lines).map(Diagram::Seq),
        "flow" => flow::parse(&lines).map(Diagram::Flow),
        "layers" => layers::parse(&lines).map(Diagram::Layers),
        "er" => er::parse(&lines).map(Diagram::Er),
        "class" => class::parse(&lines).map(Diagram::Class),
        "timeline" => timeline::parse(&lines).map(Diagram::Timeline),
        "compare" => compare::parse(&lines).map(Diagram::Compare),
        _ => return None,
    })
}

/// 図を HTML 片にする。文字はすべてエスケープする
pub fn render(diagram: &Diagram) -> String {
    let (kind, body) = match diagram {
        Diagram::Seq(d) => ("seq", seq::render(d)),
        Diagram::Flow(d) => ("flow", flow::render(d)),
        Diagram::Layers(d) => ("layers", layers::render(d)),
        Diagram::Er(d) => ("er", er::render(d)),
        Diagram::Class(d) => ("class", class::render(d)),
        Diagram::Timeline(d) => ("timeline", timeline::render(d)),
        Diagram::Compare(d) => ("compare", compare::render(d)),
    };
    format!("<div class=\"ikd ikd-{kind}\">{body}</div>")
}

/// [`render`] の結果を図の枠に入れる。`caption` が空でなければ「図{number}」と説明を添える
pub fn figure(body: &str, number: usize, caption: &str) -> String {
    let caption = caption.trim();
    let caption = if caption.is_empty() {
        String::new()
    } else {
        format!("<figcaption><b>図{number}</b>{}</figcaption>", esc(caption))
    };
    format!("<figure class=\"ikd-fig\"><div class=\"ikd-fig-body\">{body}</div>{caption}</figure>")
}

/// HTML に埋め込む文字列のエスケープ
pub fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// 空行とコメントを除いた1行
struct Line<'a> {
    no: usize,
    indented: bool,
    text: &'a str,
}

fn lines(src: &str) -> Vec<Line<'_>> {
    src.lines()
        .enumerate()
        .filter_map(|(i, raw)| {
            let text = raw.trim();
            (!text.is_empty() && !text.starts_with('#')).then(|| Line {
                no: i + 1,
                indented: raw.starts_with([' ', '\t', '\u{3000}']),
                text,
            })
        })
        .collect()
}

/// 最初の `:`（全角の `：` も）で2つに分ける。両側は前後の空白を除く
fn split_label(text: &str) -> Option<(&str, &str)> {
    let at = text.find([':', '：'])?;
    let sep = text[at..].chars().next().expect("found").len_utf8();
    Some((text[..at].trim(), text[at + sep..].trim()))
}

/// `名前 | 補足` を分ける。`|` が無ければ補足は空
fn split_note(text: &str) -> (&str, &str) {
    match text.split_once('|') {
        Some((name, note)) => (name.trim(), note.trim()),
        None => (text.trim(), ""),
    }
}

/// 図の中身が1行もないときの誤り
fn empty(what: &str) -> Vec<ParseError> {
    vec![ParseError::new(1, format!("{what}がありません"))]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_language_is_not_a_diagram() {
        assert!(parse("rust", "fn main() {}").is_none());
        assert!(parse("mermaid", "graph LR").is_none());
    }

    #[test]
    fn every_language_parses_and_renders() {
        let samples = [
            ("seq", "A -> B: x"),
            ("flow", "a -> b"),
            ("layers", "上: a"),
            ("er", "E\n  *id: int"),
            ("class", "A"),
            ("timeline", "2026-10-23: x"),
            ("compare", "A\n  + x"),
        ];
        assert_eq!(samples.len(), LANGS.len());
        for (lang, src) in samples {
            let diagram = parse(lang, src).unwrap().unwrap();
            let html = render(&diagram);
            assert!(
                html.starts_with(&format!("<div class=\"ikd ikd-{lang}\">")),
                "{html}"
            );
        }
    }

    #[test]
    fn empty_blocks_are_errors() {
        for lang in LANGS {
            let errors = parse(lang, "# コメントだけ\n\n").unwrap().unwrap_err();
            assert_eq!(errors[0].line, 1, "{lang}");
        }
    }

    #[test]
    fn lines_skip_blanks_and_comments_and_keep_numbers() {
        let lines = lines("# c\n\na\n  b\n\u{3000}c");
        let got: Vec<_> = lines.iter().map(|l| (l.no, l.indented, l.text)).collect();
        assert_eq!(got, [(3, false, "a"), (4, true, "b"), (5, true, "c")]);
    }

    #[test]
    fn split_label_accepts_full_width_colon() {
        assert_eq!(split_label("a ： b: c"), Some(("a", "b: c")));
        assert_eq!(split_label("a: b"), Some(("a", "b")));
        assert_eq!(split_label("ab"), None);
    }

    #[test]
    fn figure_adds_a_numbered_caption_only_when_given() {
        assert_eq!(
            figure("<p>x</p>", 2, " 流れ <b> "),
            "<figure class=\"ikd-fig\"><div class=\"ikd-fig-body\"><p>x</p></div><figcaption><b>図2</b>流れ &lt;b&gt;</figcaption></figure>"
        );
        assert!(!figure("x", 1, "").contains("figcaption"));
    }

    #[test]
    fn errors_display_with_line_numbers() {
        assert_eq!(ParseError::new(3, "だめ").to_string(), "3行目: だめ");
    }
}
