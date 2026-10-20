//! docx・pptx の XML を読むための小さな木。名前空間の接頭辞（`w:` など）は落として要素名だけで扱う

use anyhow::{Context, Result};
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

pub enum Node {
    Element(Element),
    Text(String),
}

#[derive(Default)]
pub struct Element {
    pub name: String,
    attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

impl Element {
    /// 接頭辞を除いた名前の属性値
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// 直下の子要素
    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|node| match node {
            Node::Element(el) => Some(el),
            Node::Text(_) => None,
        })
    }

    /// 直下の子要素のうち、名前が一致する最初のもの
    pub fn child(&self, name: &str) -> Option<&Element> {
        self.elements().find(|el| el.name == name)
    }

    /// 子孫（自分を含まない）を深さ優先でたどり、名前が一致するものを集める
    pub fn descendants<'a>(&'a self, name: &str, out: &mut Vec<&'a Element>) {
        for el in self.elements() {
            if el.name == name {
                out.push(el);
            }
            el.descendants(name, out);
        }
    }
}

fn local(name: &str) -> String {
    match name.rsplit_once(':') {
        Some((_, local)) => local.to_string(),
        None => name.to_string(),
    }
}

fn element(start: &BytesStart) -> Result<Element> {
    let mut attrs = Vec::new();
    for attr in start.attributes() {
        let attr = attr?;
        attrs.push((
            local(attr.key.as_ref()),
            attr.normalized_value(XmlVersion::Implicit1_0)?.into_owned(),
        ));
    }
    Ok(Element {
        name: local(start.name().as_ref()),
        attrs,
        children: Vec::new(),
    })
}

/// XML 全体を木にする。根の要素の子に文書の要素が並ぶ（根は名前なし）
pub fn parse(text: &str) -> Result<Element> {
    let mut reader = Reader::from_str(text);
    let mut stack = vec![Element::default()];
    let push_text = |stack: &mut Vec<Element>, text: &str| {
        let top = stack.last_mut().expect("根は残る");
        if let Some(Node::Text(last)) = top.children.last_mut() {
            last.push_str(text);
        } else {
            top.children.push(Node::Text(text.to_string()));
        }
    };
    loop {
        match reader.read_event().context("XML を読めません")? {
            Event::Start(start) => stack.push(element(&start)?),
            Event::Empty(start) => {
                let el = element(&start)?;
                stack
                    .last_mut()
                    .expect("根は残る")
                    .children
                    .push(Node::Element(el));
            }
            Event::End(_) => {
                if stack.len() > 1 {
                    let el = stack.pop().expect("長さを確かめた");
                    stack
                        .last_mut()
                        .expect("根は残る")
                        .children
                        .push(Node::Element(el));
                }
            }
            Event::Text(text) => push_text(&mut stack, &text.xml10_content()),
            Event::CData(data) => push_text(&mut stack, &data.into_inner()),
            Event::GeneralRef(reference) => {
                let resolved = match reference.resolve_char_ref()? {
                    Some(c) => c.to_string(),
                    None => match reference.into_inner().as_ref() {
                        "lt" => "<".into(),
                        "gt" => ">".into(),
                        "amp" => "&".into(),
                        "quot" => "\"".into(),
                        "apos" => "'".into(),
                        other => format!("&{other};"),
                    },
                };
                push_text(&mut stack, &resolved);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    while stack.len() > 1 {
        let el = stack.pop().expect("長さを確かめた");
        stack
            .last_mut()
            .expect("根は残る")
            .children
            .push(Node::Element(el));
    }
    Ok(stack.pop().expect("根は残る"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_drops_prefixes_and_resolves_references() {
        let root = parse(r#"<w:p a:id="1"><w:t>A &amp; B&#x30;</w:t><w:br/></w:p>"#).unwrap();
        let p = root.child("p").unwrap();
        assert_eq!(p.attr("id"), Some("1"));
        let Node::Text(text) = &p.child("t").unwrap().children[0] else {
            panic!("テキストがない");
        };
        assert_eq!(text, "A & B0");
        assert!(p.child("br").is_some());
    }
}
