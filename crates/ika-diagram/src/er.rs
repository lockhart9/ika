//! `er`: ER 図。字下げしない行がエンティティ（`表示名 英名`）、字下げした行が属性（`*名前: 型`。`*` は主キー）、
//! `A 多重度 -- 多重度 B: 関係名` が関係。エンティティは書いた順に横一列に並べ、線で結ぶのは隣り合う箱だけ。
//! 隣り合わない関係は図の下に文字で並べる。
//!
//! ```text
//! プロダクト Project
//!   *name: string
//! ノート Note
//!   *path: string
//! プロダクト 1 -- 0..* ノート: 属する
//! ```

use crate::{Line, ParseError, empty, esc, split_label};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Er {
    pub entities: Vec<Entity>,
    pub relations: Vec<Relation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entity {
    pub name: String,
    /// 英名（省略可）
    pub english: String,
    pub attrs: Vec<Attr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attr {
    pub name: String,
    pub ty: String,
    pub key: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relation {
    /// エンティティの番号
    pub from: usize,
    pub to: usize,
    pub from_mult: String,
    pub to_mult: String,
    pub name: String,
}

const SHAPE: &str = "関係は「A 1 -- 0..* B: 関係名」の形で書きます";

pub(crate) fn parse(lines: &[Line]) -> Result<Er, Vec<ParseError>> {
    let mut er = Er::default();
    let mut errors = Vec::new();
    // 関係はエンティティを全部読んでから結ぶ（後ろで宣言したエンティティも引ける）
    let mut pending = Vec::new();
    for line in lines {
        let result = if line.indented {
            attr(line.text).and_then(|attr| match er.entities.last_mut() {
                Some(entity) => {
                    entity.attrs.push(attr);
                    Ok(())
                }
                None => Err("属性の前にエンティティを書きます".into()),
            })
        } else if line.text.contains("--") {
            pending.push(line);
            Ok(())
        } else {
            entity(line.text, &er.entities).map(|e| er.entities.push(e))
        };
        if let Err(message) = result {
            errors.push(ParseError::new(line.no, message));
        }
    }
    for line in pending {
        match relation(line.text, &er.entities) {
            Ok(relation) => er.relations.push(relation),
            Err(message) => errors.push(ParseError::new(line.no, message)),
        }
    }
    if !errors.is_empty() {
        errors.sort_by_key(|e| e.line);
        return Err(errors);
    }
    if er.entities.is_empty() {
        return Err(empty("エンティティ"));
    }
    Ok(er)
}

fn entity(text: &str, existing: &[Entity]) -> Result<Entity, String> {
    let words: Vec<_> = text.split_whitespace().collect();
    let (name, english) = match words.split_last() {
        Some((last, rest)) if !rest.is_empty() && is_ident(last) => (rest.join(" "), *last),
        _ => (words.join(" "), ""),
    };
    if existing
        .iter()
        .any(|e| e.name == name || (!english.is_empty() && e.english == english))
    {
        return Err(format!("エンティティ「{name}」が重複しています"));
    }
    Ok(Entity {
        name,
        english: english.to_string(),
        attrs: Vec::new(),
    })
}

fn is_ident(word: &str) -> bool {
    word.chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && word.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn attr(text: &str) -> Result<Attr, String> {
    let (key, text) = match text.strip_prefix('*') {
        Some(rest) => (true, rest.trim()),
        None => (false, text),
    };
    let (name, ty) = split_label(text).unwrap_or((text, ""));
    if name.is_empty() {
        return Err("属性の名前を書きます（「名前: 型」）".into());
    }
    Ok(Attr {
        name: name.to_string(),
        ty: ty.to_string(),
        key,
    })
}

fn relation(text: &str, entities: &[Entity]) -> Result<Relation, String> {
    let (head, name) = split_label(text).unwrap_or((text, ""));
    let (left, right) = head.split_once("--").ok_or(SHAPE)?;
    let (from, from_mult) = left.trim().rsplit_once(char::is_whitespace).ok_or(SHAPE)?;
    let (to_mult, to) = right.trim().split_once(char::is_whitespace).ok_or(SHAPE)?;
    let find = |name: &str| {
        let name = name.trim();
        entities
            .iter()
            .position(|e| e.name == name || e.english == name)
            .ok_or_else(|| format!("エンティティ「{name}」がありません"))
    };
    for mult in [from_mult, to_mult] {
        if !is_multiplicity(mult) {
            return Err(format!(
                "多重度は 1・0..1・0..*・1..* などで書きます: {mult}"
            ));
        }
    }
    Ok(Relation {
        from: find(from)?,
        to: find(to)?,
        from_mult: from_mult.to_string(),
        to_mult: to_mult.to_string(),
        name: name.to_string(),
    })
}

fn is_multiplicity(text: &str) -> bool {
    !text.is_empty()
        && text.split("..").count() <= 2
        && text
            .split("..")
            .all(|part| part == "*" || part == "n" || part == "m" || part.parse::<u32>().is_ok())
}

pub(crate) fn render(er: &Er) -> String {
    let mut drawn = vec![false; er.relations.len()];
    let mut out = String::from("<div class=\"ikd-er-row\">");
    for (i, entity) in er.entities.iter().enumerate() {
        if i > 0 {
            // 左の箱 i-1 と右の箱 i を結ぶ関係（向きはどちらでもよい）
            let found = er.relations.iter().enumerate().find(|(r, rel)| {
                !drawn[*r]
                    && ((rel.from == i - 1 && rel.to == i) || (rel.from == i && rel.to == i - 1))
            });
            match found {
                Some((r, rel)) => {
                    drawn[r] = true;
                    let (left, right) = if rel.from == i - 1 {
                        (&rel.from_mult, &rel.to_mult)
                    } else {
                        (&rel.to_mult, &rel.from_mult)
                    };
                    out.push_str(&format!(
                        "<div class=\"ikd-rel\"><span class=\"ikd-mu ikd-a\">{}</span><span class=\"ikd-mu ikd-b\">{}</span><span class=\"ikd-nm\">{}</span></div>",
                        esc(left),
                        esc(right),
                        esc(&rel.name)
                    ));
                }
                None => out.push_str("<div class=\"ikd-rel ikd-none\"></div>"),
            }
        }
        out.push_str(&entity_box(entity));
    }
    out.push_str("</div>");

    let rest: Vec<_> = er
        .relations
        .iter()
        .zip(&drawn)
        .filter(|(_, drawn)| !**drawn)
        .map(|(rel, _)| rel)
        .collect();
    if !rest.is_empty() {
        out.push_str("<ul class=\"ikd-rels\">");
        for rel in rest {
            let name = if rel.name.is_empty() {
                String::new()
            } else {
                format!("：{}", esc(&rel.name))
            };
            out.push_str(&format!(
                "<li>{} <i>{}</i> ― <i>{}</i> {}{name}</li>",
                esc(&er.entities[rel.from].name),
                esc(&rel.from_mult),
                esc(&rel.to_mult),
                esc(&er.entities[rel.to].name),
            ));
        }
        out.push_str("</ul>");
    }
    out
}

fn entity_box(entity: &Entity) -> String {
    let english = if entity.english.is_empty() {
        String::new()
    } else {
        format!("<em>{}</em>", esc(&entity.english))
    };
    let mut out = format!(
        "<div class=\"ikd-ent\"><h4>{}{english}</h4>",
        esc(&entity.name)
    );
    if !entity.attrs.is_empty() {
        out.push_str("<ul>");
        for attr in &entity.attrs {
            let key = if attr.key { "<u>PK</u>" } else { "" };
            out.push_str(&format!(
                "<li><span>{key}{}</span><i>{}</i></li>",
                esc(&attr.name),
                esc(&attr.ty)
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

    fn parse_src(src: &str) -> Result<Er, Vec<ParseError>> {
        parse(&lines(src))
    }

    const SAMPLE: &str = "プロダクト Project\n  *name: string\n  status: enum\nノート Note\n  *path: string\nタグ Tag\n  *name: string\n\nプロダクト 1 -- 0..* ノート: 属する\nTag 0..* -- 0..* Note: 付ける\nプロダクト 1 -- 0..* タグ\n";

    #[test]
    fn parses_entities_attributes_and_relations_by_either_name() {
        let er = parse_src(SAMPLE).unwrap();
        assert_eq!(er.entities.len(), 3);
        assert_eq!(er.entities[0].name, "プロダクト");
        assert_eq!(er.entities[0].english, "Project");
        assert!(er.entities[0].attrs[0].key);
        assert_eq!(er.entities[0].attrs[1].ty, "enum");
        assert_eq!(
            er.relations[1],
            Relation {
                from: 2,
                to: 1,
                from_mult: "0..*".into(),
                to_mult: "0..*".into(),
                name: "付ける".into()
            }
        );
    }

    #[test]
    fn rejects_unknown_entities_bad_multiplicities_and_orphan_attributes() {
        let errors = parse_src("  id: int\nA\nA\nA 1 -- 0..* B\nA x -- 1 A\nA -- A\n").unwrap_err();
        let lines: Vec<_> = errors.iter().map(|e| e.line).collect();
        assert_eq!(lines, [1, 3, 4, 5, 6], "{errors:?}");
    }

    #[test]
    fn draws_adjacent_relations_and_lists_the_rest() {
        let html = render(&parse_src(SAMPLE).unwrap());
        // プロダクト─ノート（1 … 0..*）、ノート─タグは逆向きに書いても左右を合わせる
        assert!(html.contains("<div class=\"ikd-rel\"><span class=\"ikd-mu ikd-a\">1</span><span class=\"ikd-mu ikd-b\">0..*</span><span class=\"ikd-nm\">属する</span></div>"));
        assert!(html.contains("<span class=\"ikd-nm\">付ける</span>"));
        assert!(html.contains("<li><span><u>PK</u>name</span><i>string</i></li>"));
        assert!(html.contains(
            "<ul class=\"ikd-rels\"><li>プロダクト <i>1</i> ― <i>0..*</i> タグ</li></ul>"
        ));
    }

    #[test]
    fn leaves_a_gap_between_unrelated_neighbours() {
        let html = render(&parse_src("A\nB").unwrap());
        assert!(html.contains("<div class=\"ikd-rel ikd-none\"></div>"));
        assert!(!html.contains("ikd-rels"));
    }
}
