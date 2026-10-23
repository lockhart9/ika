//! `flow`: フロー・状態遷移。最初の行が `->` でつないだ一本の鎖で、`?` の判断で終われば、
//! 字下げした行が1行1枝の分岐になる（`ラベル: 鎖`）。合流とループは描かない。
//!
//! ```text
//! Markdown | kb/ のページ -> 図のブロックを探す -> ?記法は正しい？
//!   はい: *CSS の部品に展開 -> +単独の HTML
//!   いいえ: !行と理由を表示して止める
//! ```

use crate::{Line, ParseError, empty, esc, split_label, split_note};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Flow {
    pub chain: Vec<Node>,
    pub branches: Vec<Branch>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    pub label: String,
    pub chain: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub kind: NodeKind,
    pub name: String,
    /// `名前 | 補足` の補足（箱の下に小さく出す）
    pub note: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Plain,
    /// `?` 判断
    Decision,
    /// `*` 主役
    Key,
    /// `+` 成功
    Ok,
    /// `!` 問題
    Problem,
}

pub(crate) fn parse(lines: &[Line]) -> Result<Flow, Vec<ParseError>> {
    let mut flow = Flow::default();
    let mut errors = Vec::new();
    let mut first_branch_line = None;
    for (i, line) in lines.iter().enumerate() {
        let result = if i == 0 {
            if line.indented {
                Err("最初の行（本体の鎖）は字下げせずに書きます".to_string())
            } else {
                chain(line.text).map(|c| flow.chain = c)
            }
        } else if !line.indented {
            Err("本体の鎖は1行です。分岐は字下げして「ラベル: 鎖」と書きます".to_string())
        } else {
            first_branch_line.get_or_insert(line.no);
            let (label, rest) = split_label(line.text).unwrap_or(("", line.text));
            chain(rest).map(|c| {
                flow.branches.push(Branch {
                    label: label.to_string(),
                    chain: c,
                })
            })
        };
        if let Err(message) = result {
            errors.push(ParseError::new(line.no, message));
        }
    }
    if let Some(no) = first_branch_line
        && flow
            .chain
            .last()
            .is_some_and(|n| n.kind != NodeKind::Decision)
    {
        errors.push(ParseError::new(
            no,
            "分岐は、本体の鎖が「?」の判断で終わるときだけ書けます",
        ));
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    if flow.chain.is_empty() {
        return Err(empty("ノード"));
    }
    Ok(flow)
}

fn chain(text: &str) -> Result<Vec<Node>, String> {
    text.split("->").map(node).collect()
}

fn node(text: &str) -> Result<Node, String> {
    let text = text.trim();
    let mut chars = text.chars();
    let kind = match chars.next() {
        Some('?') => NodeKind::Decision,
        Some('*') => NodeKind::Key,
        Some('+') => NodeKind::Ok,
        Some('!') => NodeKind::Problem,
        _ => NodeKind::Plain,
    };
    let rest = if kind == NodeKind::Plain {
        text
    } else {
        chars.as_str()
    };
    let (name, note) = split_note(rest);
    if name.is_empty() {
        return Err("空のノードがあります（-> の前後に名前を書きます）".into());
    }
    Ok(Node {
        kind,
        name: name.to_string(),
        note: note.to_string(),
    })
}

pub(crate) fn render(flow: &Flow) -> String {
    let mut out = String::from("<div class=\"ikd-flow-row\">");
    push_chain(&mut out, &flow.chain);
    if !flow.branches.is_empty() {
        out.push_str("<div class=\"ikd-fork\">");
        for branch in &flow.branches {
            out.push_str(&format!("<div class=\"ikd-lane\">{}", arrow(&branch.label)));
            push_chain(&mut out, &branch.chain);
            out.push_str("</div>");
        }
        out.push_str("</div>");
    }
    out.push_str("</div>");
    out
}

fn push_chain(out: &mut String, chain: &[Node]) {
    for (i, node) in chain.iter().enumerate() {
        if i > 0 {
            out.push_str(&arrow(""));
        }
        let class = match node.kind {
            NodeKind::Plain => "ikd-node",
            NodeKind::Decision => "ikd-node ikd-q",
            NodeKind::Key => "ikd-node ikd-key",
            NodeKind::Ok => "ikd-node ikd-ok",
            NodeKind::Problem => "ikd-node ikd-bad",
        };
        let note = if node.note.is_empty() {
            String::new()
        } else {
            format!("<small>{}</small>", esc(&node.note))
        };
        out.push_str(&format!(
            "<div class=\"{class}\">{}{note}</div>",
            esc(&node.name)
        ));
    }
}

fn arrow(label: &str) -> String {
    if label.is_empty() {
        "<span class=\"ikd-arrow\"></span>".into()
    } else {
        format!("<span class=\"ikd-arrow\"><b>{}</b></span>", esc(label))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lines;

    fn parse_src(src: &str) -> Result<Flow, Vec<ParseError>> {
        parse(&lines(src))
    }

    #[test]
    fn parses_chain_markers_notes_and_branches() {
        let flow = parse_src(
            "Markdown | kb/ のページ -> 探す -> ?正しい？\n  はい: *展開 -> +HTML\n  いいえ：!止める\n",
        )
        .unwrap();
        let kinds: Vec<_> = flow.chain.iter().map(|n| n.kind).collect();
        assert_eq!(
            kinds,
            [NodeKind::Plain, NodeKind::Plain, NodeKind::Decision]
        );
        assert_eq!(flow.chain[0].note, "kb/ のページ");
        assert_eq!(flow.chain[2].name, "正しい？");
        assert_eq!(flow.branches.len(), 2);
        assert_eq!(flow.branches[0].label, "はい");
        assert_eq!(flow.branches[0].chain[1].kind, NodeKind::Ok);
        assert_eq!(flow.branches[1].label, "いいえ");
        assert_eq!(flow.branches[1].chain[0].kind, NodeKind::Problem);
    }

    #[test]
    fn rejects_branches_without_decision_empty_nodes_and_second_bodies() {
        let errors = parse_src("a -> b\n  x: c\n").unwrap_err();
        assert_eq!(errors[0].line, 2);
        assert!(errors[0].message.contains("判断"));

        let errors = parse_src("a -> -> b\nc -> d\n").unwrap_err();
        let lines: Vec<_> = errors.iter().map(|e| e.line).collect();
        assert_eq!(lines, [1, 2]);

        let errors = parse_src("  a -> b").unwrap_err();
        assert!(errors[0].message.contains("字下げせず"));
    }

    #[test]
    fn renders_nodes_arrows_and_lanes() {
        let html = render(&parse_src("a | 補足 -> ?q\n  はい: *k\n  !n").unwrap());
        assert_eq!(
            html,
            "<div class=\"ikd-flow-row\">\
             <div class=\"ikd-node\">a<small>補足</small></div><span class=\"ikd-arrow\"></span>\
             <div class=\"ikd-node ikd-q\">q</div>\
             <div class=\"ikd-fork\">\
             <div class=\"ikd-lane\"><span class=\"ikd-arrow\"><b>はい</b></span><div class=\"ikd-node ikd-key\">k</div></div>\
             <div class=\"ikd-lane\"><span class=\"ikd-arrow\"></span><div class=\"ikd-node ikd-bad\">n</div></div>\
             </div></div>"
        );
    }
}
