//! `class`: クラス図（継承の木）。字下げしない行がクラス（`名前 : 親 <<abstract>>`）、
//! 字下げした行が属性・操作で、`--` で区画を分ける。クラスどうしの関連（持つ・使う）は描かない。
//!
//! ```text
//! Diagram <<abstract>>
//!   + source: String
//!   --
//!   + render(): Html
//! Seq : Diagram
//! ```

use crate::{Line, ParseError, empty, esc, split_label};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Classes {
    pub classes: Vec<Class>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Class {
    pub name: String,
    /// 親クラスの番号
    pub parent: Option<usize>,
    /// `<<...>>` の中身（`abstract` なら名前を斜体にする）
    pub stereotype: String,
    /// `--` で分けた区画。ふつうは属性・操作の2つ
    pub compartments: Vec<Vec<Member>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// `+ parse()` のような、型を除いた部分
    pub text: String,
    pub ty: String,
}

pub(crate) fn parse(lines: &[Line]) -> Result<Classes, Vec<ParseError>> {
    let mut classes = Classes::default();
    let mut parents = Vec::new();
    let mut errors = Vec::new();
    for line in lines {
        let result = if line.indented {
            match classes.classes.last_mut() {
                None => Err("属性・操作の前にクラスを書きます".to_string()),
                Some(class) if line.text == "--" => {
                    class.compartments.push(Vec::new());
                    Ok(())
                }
                Some(class) => {
                    class
                        .compartments
                        .last_mut()
                        .expect("1つは必ずある")
                        .push(member(line.text));
                    Ok(())
                }
            }
        } else {
            header(line.text, &classes.classes).map(|(class, parent)| {
                classes.classes.push(class);
                parents.push((line.no, parent));
            })
        };
        if let Err(message) = result {
            errors.push(ParseError::new(line.no, message));
        }
    }
    // 親は全部読んでから引く（後ろで宣言した親も書ける）
    for (i, (no, parent)) in parents.iter().enumerate() {
        let Some(parent) = parent else { continue };
        match classes.classes.iter().position(|c| &c.name == parent) {
            Some(p) if p == i => errors.push(ParseError::new(*no, "自分自身は継承できません")),
            Some(p) => classes.classes[i].parent = Some(p),
            None => errors.push(ParseError::new(
                *no,
                format!("親クラス「{parent}」がありません"),
            )),
        }
    }
    if errors.is_empty() {
        for (i, (no, _)) in parents.iter().enumerate() {
            if cycles(&classes.classes, i) {
                errors.push(ParseError::new(*no, "継承が循環しています"));
            }
        }
    }
    if !errors.is_empty() {
        errors.sort_by_key(|e| e.line);
        return Err(errors);
    }
    if classes.classes.is_empty() {
        return Err(empty("クラス"));
    }
    Ok(classes)
}

fn header(text: &str, existing: &[Class]) -> Result<(Class, Option<String>), String> {
    let (text, stereotype) = match (text.find("<<"), text.find(">>")) {
        (Some(open), Some(close)) if open < close => (
            format!("{}{}", &text[..open], &text[close + 2..]),
            text[open + 2..close].trim().to_string(),
        ),
        (None, None) => (text.to_string(), String::new()),
        _ => return Err("ステレオタイプは <<abstract>> のように書きます".into()),
    };
    let (name, parent) = match split_label(&text) {
        Some((name, parent)) => {
            if parent.is_empty() {
                return Err("「:」のあとに親クラスを書きます".into());
            }
            (name.to_string(), Some(parent.to_string()))
        }
        None => (text.trim().to_string(), None),
    };
    if name.is_empty() {
        return Err("クラス名を書きます".into());
    }
    if existing.iter().any(|c| c.name == name) {
        return Err(format!("クラス「{name}」が重複しています"));
    }
    Ok((
        Class {
            name,
            parent: None,
            stereotype,
            compartments: vec![Vec::new()],
        },
        parent,
    ))
}

/// 型は最後の `:` のあと（`+ parse(): Result` の `Result`）
fn member(text: &str) -> Member {
    match text.rfind([':', '：']) {
        Some(at) => {
            let sep = text[at..].chars().next().expect("found").len_utf8();
            Member {
                text: text[..at].trim().to_string(),
                ty: text[at + sep..].trim().to_string(),
            }
        }
        None => Member {
            text: text.to_string(),
            ty: String::new(),
        },
    }
}

fn cycles(classes: &[Class], start: usize) -> bool {
    let mut current = classes[start].parent;
    for _ in 0..classes.len() {
        match current {
            Some(i) if i == start => return true,
            Some(i) => current = classes[i].parent,
            None => return false,
        }
    }
    true
}

pub(crate) fn render(classes: &Classes) -> String {
    let mut out = String::from("<div class=\"ikd-forest\">");
    for (i, class) in classes.classes.iter().enumerate() {
        if class.parent.is_none() {
            tree(&mut out, classes, i);
        }
    }
    out.push_str("</div>");
    out
}

fn tree(out: &mut String, classes: &Classes, i: usize) {
    out.push_str("<div class=\"ikd-tree\">");
    out.push_str(&class_box(&classes.classes[i]));
    let children: Vec<_> = (0..classes.classes.len())
        .filter(|&c| classes.classes[c].parent == Some(i))
        .collect();
    if !children.is_empty() {
        out.push_str("<div class=\"ikd-stem\"></div><div class=\"ikd-kids\">");
        for child in children {
            out.push_str("<div class=\"ikd-kid\">");
            tree(out, classes, child);
            out.push_str("</div>");
        }
        out.push_str("</div>");
    }
    out.push_str("</div>");
}

fn class_box(class: &Class) -> String {
    let (h4, label) = match class.stereotype.as_str() {
        "" => ("<h4>", String::new()),
        "abstract" => ("<h4 class=\"ikd-abs\">", "<em>abstract</em>".to_string()),
        other => ("<h4>", format!("<em>«{}»</em>", esc(other))),
    };
    let mut out = format!(
        "<div class=\"ikd-ent ikd-cls\">{h4}{}{label}</h4>",
        esc(&class.name)
    );
    for compartment in class.compartments.iter().filter(|c| !c.is_empty()) {
        out.push_str("<ul>");
        for member in compartment {
            out.push_str(&format!(
                "<li><span>{}</span><i>{}</i></li>",
                esc(&member.text),
                esc(&member.ty)
            ));
        }
        out.push_str("</ul>");
    }
    out.push_str("</div>");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lines;

    fn parse_src(src: &str) -> Result<Classes, Vec<ParseError>> {
        parse(&lines(src))
    }

    #[test]
    fn parses_inheritance_stereotypes_and_compartments() {
        let classes = parse_src(
            "Seq : Diagram\n  - actors: Vec\nDiagram <<abstract>>\n  + source: String\n  --\n  + parse(): Result\n",
        )
        .unwrap();
        assert_eq!(classes.classes[0].parent, Some(1));
        assert_eq!(classes.classes[1].stereotype, "abstract");
        assert_eq!(
            classes.classes[1].compartments,
            [
                vec![Member {
                    text: "+ source".into(),
                    ty: "String".into()
                }],
                vec![Member {
                    text: "+ parse()".into(),
                    ty: "Result".into()
                }],
            ]
        );
    }

    #[test]
    fn rejects_missing_parents_cycles_and_duplicates() {
        let errors = parse_src("A : B").unwrap_err();
        assert!(errors[0].message.contains("「B」がありません"));
        let errors = parse_src("A : B\nB : A").unwrap_err();
        assert!(errors[0].message.contains("循環"));
        let errors = parse_src("  x\nA\nA\nC : C\nD <<x").unwrap_err();
        let lines: Vec<_> = errors.iter().map(|e| e.line).collect();
        assert_eq!(lines, [1, 3, 4, 5], "{errors:?}");
    }

    #[test]
    fn renders_nested_trees_with_roots_side_by_side() {
        let html = render(&parse_src("A <<abstract>>\nB : A\nC : B\nD").unwrap());
        assert_eq!(
            html,
            "<div class=\"ikd-forest\">\
             <div class=\"ikd-tree\"><div class=\"ikd-ent ikd-cls\"><h4 class=\"ikd-abs\">A<em>abstract</em></h4></div>\
             <div class=\"ikd-stem\"></div><div class=\"ikd-kids\"><div class=\"ikd-kid\">\
             <div class=\"ikd-tree\"><div class=\"ikd-ent ikd-cls\"><h4>B</h4></div>\
             <div class=\"ikd-stem\"></div><div class=\"ikd-kids\"><div class=\"ikd-kid\">\
             <div class=\"ikd-tree\"><div class=\"ikd-ent ikd-cls\"><h4>C</h4></div></div>\
             </div></div></div></div></div></div>\
             <div class=\"ikd-tree\"><div class=\"ikd-ent ikd-cls\"><h4>D</h4></div></div>\
             </div>"
        );
    }
}
