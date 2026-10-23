//! `layers`: 層の構成図。上から順に、上の段が下の段を使う。1行が1段で、`, ` で区切った語が札になる。
//!
//! ```text
//! AI クライアント: Claude Code, Codex
//! CLI {new}: ika (Rust)
//! 正本 | 変えない {keep}: kb の Markdown, data の CSV
//! ```

use crate::{Line, ParseError, empty, esc, split_label, split_note};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Layers {
    pub layers: Vec<Layer>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layer {
    pub name: String,
    pub note: String,
    pub tone: Tone,
    pub items: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Plain,
    /// `{new}` 新しく足す（藍）
    New,
    /// `{keep}` 変えない（苔）
    Keep,
}

pub(crate) fn parse(lines: &[Line]) -> Result<Layers, Vec<ParseError>> {
    let mut layers = Layers::default();
    let mut errors = Vec::new();
    for line in lines {
        match layer(line) {
            Ok(layer) => layers.layers.push(layer),
            Err(message) => errors.push(ParseError::new(line.no, message)),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    if layers.layers.is_empty() {
        return Err(empty("段"));
    }
    Ok(layers)
}

fn layer(line: &Line) -> Result<Layer, String> {
    if line.indented {
        return Err("段は字下げせずに1行ずつ書きます".into());
    }
    let (head, items) = split_label(line.text).unwrap_or((line.text, ""));
    let (head, tone) = match head.find('{') {
        Some(open) => {
            let close = head[open..]
                .find('}')
                .map(|c| open + c)
                .ok_or("{new} か {keep} を閉じてください")?;
            let tone = match head[open + 1..close].trim() {
                "new" => Tone::New,
                "keep" => Tone::Keep,
                other => return Err(format!("色は {{new}} か {{keep}} です: {{{other}}}")),
            };
            (format!("{}{}", &head[..open], &head[close + 1..]), tone)
        }
        None => (head.to_string(), Tone::Plain),
    };
    let (name, note) = split_note(&head);
    if name.is_empty() {
        return Err("段の名前を書きます（「名前: 札, 札」）".into());
    }
    Ok(Layer {
        name: name.to_string(),
        note: note.to_string(),
        tone,
        items: items
            .split([',', '、'])
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect(),
    })
}

pub(crate) fn render(layers: &Layers) -> String {
    let mut out = String::new();
    for layer in &layers.layers {
        let class = match layer.tone {
            Tone::Plain => "ikd-layer",
            Tone::New => "ikd-layer ikd-new",
            Tone::Keep => "ikd-layer ikd-keep",
        };
        let note = if layer.note.is_empty() {
            String::new()
        } else {
            format!("<small>{}</small>", esc(&layer.note))
        };
        let chips: String = layer
            .items
            .iter()
            .map(|item| format!("<span>{}</span>", esc(item)))
            .collect();
        out.push_str(&format!(
            "<div class=\"{class}\"><b>{}{note}</b><div class=\"ikd-chips\">{chips}</div></div>",
            esc(&layer.name)
        ));
    }
    // 色を使ったときだけ凡例を出す
    let mut legend = String::new();
    for (tone, label) in [(Tone::New, "新しく足す"), (Tone::Keep, "変えない")] {
        if layers.layers.iter().any(|l| l.tone == tone) {
            let class = if tone == Tone::New { "new" } else { "keep" };
            legend.push_str(&format!("<span class=\"ikd-sw-{class}\">{label}</span>"));
        }
    }
    if !legend.is_empty() {
        out.push_str(&format!("<div class=\"ikd-legend\">{legend}</div>"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lines;

    fn parse_src(src: &str) -> Result<Layers, Vec<ParseError>> {
        parse(&lines(src))
    }

    #[test]
    fn parses_names_notes_tones_and_items() {
        let layers = parse_src(
            "AI クライアント: Claude Code, Codex\nCLI {new}: ika (Rust)\n正本 | 変えない {keep}：kb、data\n空\n",
        )
        .unwrap();
        let got: Vec<_> = layers
            .layers
            .iter()
            .map(|l| (l.name.as_str(), l.note.as_str(), l.tone, l.items.len()))
            .collect();
        assert_eq!(
            got,
            [
                ("AI クライアント", "", Tone::Plain, 2),
                ("CLI", "", Tone::New, 1),
                ("正本", "変えない", Tone::Keep, 2),
                ("空", "", Tone::Plain, 0),
            ]
        );
    }

    #[test]
    fn rejects_unknown_tones_indents_and_missing_names() {
        let errors = parse_src("a {red}: x\n  b: y\n: z\n").unwrap_err();
        let lines: Vec<_> = errors.iter().map(|e| e.line).collect();
        assert_eq!(lines, [1, 2, 3]);
    }

    #[test]
    fn renders_layers_and_a_legend_only_for_used_tones() {
        let html = render(&parse_src("上 | 補足 {new}: a, b\n下: c").unwrap());
        assert_eq!(
            html,
            "<div class=\"ikd-layer ikd-new\"><b>上<small>補足</small></b><div class=\"ikd-chips\"><span>a</span><span>b</span></div></div>\
             <div class=\"ikd-layer\"><b>下</b><div class=\"ikd-chips\"><span>c</span></div></div>\
             <div class=\"ikd-legend\"><span class=\"ikd-sw-new\">新しく足す</span></div>"
        );
        assert!(!render(&parse_src("a: b").unwrap()).contains("legend"));
    }
}
