//! `tree`: 木（階層）。1行が1節で、上の行より深く字下げした行がその子になる。根は複数でもよい。
//! 印は `flow` と同じ `*` 主役・`+` 成功・`!` 問題で、`名前 | 補足` も書ける。
//!
//! ```text
//! / | トップ
//!   /characters
//!     /characters/[id]
//!   *マテリア | 一番読まれる
//!   !旧ページ | 消す予定
//! ```

use crate::{Line, Mark, ParseError, empty, esc, mark, split_note};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tree {
    pub roots: Vec<TreeNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeNode {
    pub mark: Mark,
    pub name: String,
    /// `名前 | 補足` の補足（名前の後ろに小さく出す）
    pub note: String,
    pub children: Vec<TreeNode>,
}

pub(crate) fn parse(lines: &[Line]) -> Result<Tree, Vec<ParseError>> {
    let mut errors = Vec::new();
    // 書いた順の（深さ, 節）。深さは祖先の字下げの幅の積み上げから決める
    let mut flat = Vec::new();
    let mut indents: Vec<usize> = Vec::new();
    for line in lines {
        let depth = match depth(&mut indents, line.indent) {
            Ok(depth) => depth,
            Err(message) => {
                errors.push(ParseError::new(line.no, message));
                continue;
            }
        };
        let (mark, rest) = mark(line.text);
        let (name, note) = split_note(rest);
        if name.is_empty() {
            errors.push(ParseError::new(line.no, "節の名前を書きます"));
            continue;
        }
        flat.push((
            depth,
            TreeNode {
                mark,
                name: name.to_string(),
                note: note.to_string(),
                children: Vec::new(),
            },
        ));
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    if flat.is_empty() {
        return Err(empty("節"));
    }
    let mut flat = flat.into_iter().peekable();
    Ok(Tree {
        roots: build(&mut flat, 0),
    })
}

/// 字下げの幅から深さを決める。`indents` は今の節までの祖先の字下げの幅
fn depth(indents: &mut Vec<usize>, indent: usize) -> Result<usize, String> {
    if indents.is_empty() {
        if indent > 0 {
            return Err("最初の行（根）は字下げせずに書きます".into());
        }
        indents.push(0);
        return Ok(0);
    }
    let mut popped = false;
    while indents.last().is_some_and(|&last| last > indent) {
        indents.pop();
        popped = true;
    }
    let last = *indents.last().expect("根は残る");
    if last == indent {
        Ok(indents.len() - 1)
    } else if popped {
        Err("字下げが、上のどの節ともそろっていません（親と同じ深さにそろえます）".into())
    } else {
        indents.push(indent);
        Ok(indents.len() - 1)
    }
}

/// 書いた順の（深さ, 節）から、`depth` の深さの兄弟を組み立てる
fn build(
    flat: &mut std::iter::Peekable<impl Iterator<Item = (usize, TreeNode)>>,
    depth: usize,
) -> Vec<TreeNode> {
    let mut siblings = Vec::new();
    while let Some((_, mut node)) = flat.next_if(|(d, _)| *d == depth) {
        if flat.peek().is_some_and(|(d, _)| *d > depth) {
            node.children = build(flat, depth + 1);
        }
        siblings.push(node);
    }
    siblings
}

pub(crate) fn render(tree: &Tree) -> String {
    let mut out = String::new();
    push_list(&mut out, &tree.roots, "ikd-tv");
    out
}

fn push_list(out: &mut String, nodes: &[TreeNode], class: &str) {
    if class.is_empty() {
        out.push_str("<ul>");
    } else {
        out.push_str(&format!("<ul class=\"{class}\">"));
    }
    for node in nodes {
        let note = if node.note.is_empty() {
            String::new()
        } else {
            format!("<small>{}</small>", esc(&node.note))
        };
        out.push_str(&format!(
            "<li><span class=\"ikd-node{}\">{}{note}</span>",
            node.mark.class(),
            esc(&node.name)
        ));
        if !node.children.is_empty() {
            push_list(out, &node.children, "");
        }
        out.push_str("</li>");
    }
    out.push_str("</ul>");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lines;

    fn parse_src(src: &str) -> Result<Tree, Vec<ParseError>> {
        parse(&lines(src))
    }

    fn names(nodes: &[TreeNode]) -> Vec<&str> {
        nodes.iter().map(|n| n.name.as_str()).collect()
    }

    #[test]
    fn builds_parents_and_children_from_indentation() {
        let tree = parse_src("a\n  b\n    c\n  d\ne").unwrap();
        assert_eq!(names(&tree.roots), ["a", "e"]);
        assert_eq!(names(&tree.roots[0].children), ["b", "d"]);
        assert_eq!(names(&tree.roots[0].children[0].children), ["c"]);
        assert!(tree.roots[1].children.is_empty());
    }

    #[test]
    fn reads_marks_notes_and_any_kind_of_indentation() {
        let tree = parse_src("*根 | 補足\n\t+済み\n\u{3000}\u{3000}!問題 | 注意\n").unwrap();
        let root = &tree.roots[0];
        assert_eq!(
            (root.mark, root.name.as_str(), root.note.as_str()),
            (Mark::Key, "根", "補足")
        );
        assert_eq!(root.children[0].mark, Mark::Ok);
        // タブ（4）と全角空白 2 つ（4）は同じ深さ
        assert_eq!(root.children[1].mark, Mark::Problem);
        assert_eq!(root.children[1].note, "注意");
    }

    #[test]
    fn rejects_indented_first_line_misaligned_dedent_and_empty_names() {
        let errors = parse_src("  a").unwrap_err();
        assert_eq!(errors[0].line, 1);
        assert!(errors[0].message.contains("根"), "{errors:?}");

        let errors = parse_src("a\n    b\n  c").unwrap_err();
        assert_eq!(errors[0].line, 3);
        assert!(errors[0].message.contains("そろって"), "{errors:?}");

        let errors = parse_src("a\n  *\n  | 補足だけ").unwrap_err();
        let lines: Vec<_> = errors.iter().map(|e| e.line).collect();
        assert_eq!(lines, [2, 3]);
    }

    #[test]
    fn renders_nested_lists_with_marks_and_escaped_text() {
        let html = render(&parse_src("<a> | n\n  *b\n    !c\n  +d").unwrap());
        assert_eq!(
            html,
            "<ul class=\"ikd-tv\"><li><span class=\"ikd-node\">&lt;a&gt;<small>n</small></span>\
             <ul><li><span class=\"ikd-node ikd-key\">b</span>\
             <ul><li><span class=\"ikd-node ikd-bad\">c</span></li></ul></li>\
             <li><span class=\"ikd-node ikd-ok\">d</span></li></ul></li></ul>"
        );
    }
}
