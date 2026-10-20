//! PowerPoint（.pptx）を Markdown にする。スライドごとに見出しを立て、文字・表・発表者ノートを残す

use anyhow::{Result, bail};

use super::convert::{Converted, core_title, open_zip, table, text_of, zip_entry};
use super::docx::relationships;
use super::xml::{self, Element};

pub fn convert(bytes: &[u8]) -> Result<Converted> {
    let mut archive = open_zip(bytes)?;
    let mut slides: Vec<(usize, String)> = archive
        .file_names()
        .filter_map(|name| {
            let name = name.ok()?;
            let n = name
                .strip_prefix("ppt/slides/slide")?
                .strip_suffix(".xml")?
                .parse()
                .ok()?;
            Some((n, name.to_string()))
        })
        .collect();
    if slides.is_empty() {
        bail!("スライドがありません（pptx ではないかもしれません）");
    }
    slides.sort();
    let title = core_title(&mut archive)?;

    let mut sections = Vec::new();
    let mut images = 0;
    for (n, name) in slides {
        let Some(text) = zip_entry(&mut archive, &name)? else {
            continue;
        };
        let root = xml::parse(&text)?;
        let mut slide = Slide::default();
        let mut trees = Vec::new();
        root.descendants("spTree", &mut trees);
        if let Some(tree) = trees.first() {
            shapes(tree, &mut slide);
        }
        images += slide.images;

        let mut section = match &slide.title {
            Some(title) => format!("## {n}. {title}"),
            None => format!("## スライド {n}"),
        };
        if !slide.blocks.is_empty() {
            section.push_str("\n\n");
            section.push_str(&slide.blocks.join("\n\n"));
        }
        let rels = format!("ppt/slides/_rels/slide{n}.xml.rels");
        if let Some(notes) = notes(&mut archive, &rels)? {
            section.push_str(&format!("\n\n> ノート: {}", notes.replace('\n', "\n> ")));
        }
        sections.push(section);
    }

    let mut warnings = Vec::new();
    if images > 0 {
        warnings.push(format!(
            "画像 {images} 個は取り込んでいません（文字だけを取り込みます）"
        ));
    }
    Ok(Converted {
        body: sections.join("\n\n"),
        title,
        warnings,
    })
}

#[derive(Default)]
struct Slide {
    title: Option<String>,
    blocks: Vec<String>,
    images: usize,
}

/// 図形を並び順にたどる（グループの中も）
fn shapes(tree: &Element, slide: &mut Slide) {
    for shape in tree.elements() {
        match shape.name.as_str() {
            "sp" => {
                let Some(body) = shape.child("txBody") else {
                    continue;
                };
                let paragraphs = paragraphs(body);
                if paragraphs.is_empty() {
                    continue;
                }
                if is_title(shape) && slide.title.is_none() {
                    let text: Vec<_> = paragraphs.into_iter().map(|(_, t)| t).collect();
                    slide.title = Some(text.join(" "));
                    continue;
                }
                let lines: Vec<_> = paragraphs
                    .into_iter()
                    .map(|(level, text)| format!("{}- {text}", "  ".repeat(level)))
                    .collect();
                slide.blocks.push(lines.join("\n"));
            }
            "graphicFrame" => {
                let mut tables = Vec::new();
                shape.descendants("tbl", &mut tables);
                for tbl in tables {
                    let rows: Vec<Vec<String>> = tbl
                        .elements()
                        .filter(|e| e.name == "tr")
                        .map(|tr| {
                            tr.elements()
                                .filter(|e| e.name == "tc")
                                .map(|tc| {
                                    let text: Vec<_> =
                                        paragraphs(tc).into_iter().map(|(_, t)| t).collect();
                                    text.join("<br>")
                                })
                                .collect()
                        })
                        .collect();
                    if !rows.is_empty() {
                        slide.blocks.push(table(&rows));
                    }
                }
            }
            "pic" => slide.images += 1,
            "grpSp" => shapes(shape, slide),
            _ => {}
        }
    }
}

fn is_title(shape: &Element) -> bool {
    let mut placeholders = Vec::new();
    shape.descendants("ph", &mut placeholders);
    placeholders
        .iter()
        .any(|ph| matches!(ph.attr("type"), Some("title" | "ctrTitle")))
}

/// テキストの段落（a:p）ごとに (字下げの深さ, 文字)。空の段落は除く
fn paragraphs(body: &Element) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    body.descendants("p", &mut found);
    found
        .into_iter()
        .filter_map(|p| {
            let mut text = String::new();
            for part in p.elements() {
                match part.name.as_str() {
                    "r" | "fld" => {
                        if let Some(t) = part.child("t") {
                            text.push_str(&text_of(t));
                        }
                    }
                    "br" => text.push(' '),
                    _ => {}
                }
            }
            let text = text.trim().to_string();
            let level = p
                .child("pPr")
                .and_then(|pr| pr.attr("lvl"))
                .and_then(|l| l.parse().ok())
                .unwrap_or(0);
            (!text.is_empty()).then_some((level, text))
        })
        .collect()
}

/// スライドに付いた発表者ノートの文字
fn notes(
    archive: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>,
    rels: &str,
) -> Result<Option<String>> {
    let Some(rels) = zip_entry(archive, rels)? else {
        return Ok(None);
    };
    let Some(target) = relationships(&rels)?
        .into_values()
        .find(|t| t.contains("notesSlide"))
    else {
        return Ok(None);
    };
    let name = format!(
        "ppt/notesSlides/{}",
        target.rsplit('/').next().unwrap_or(&target)
    );
    let Some(text) = zip_entry(archive, &name)? else {
        return Ok(None);
    };
    let root = xml::parse(&text)?;
    let mut shapes = Vec::new();
    root.descendants("sp", &mut shapes);
    // ノートの本文のプレースホルダー（type="body"）だけ。スライド番号などは除く
    let text: Vec<String> = shapes
        .into_iter()
        .filter(|sp| {
            let mut ph = Vec::new();
            sp.descendants("ph", &mut ph);
            ph.iter().any(|p| p.attr("type") == Some("body"))
        })
        .filter_map(|sp| sp.child("txBody"))
        .flat_map(|body| paragraphs(body).into_iter().map(|(_, t)| t))
        .collect();
    Ok((!text.is_empty()).then(|| text.join("\n")))
}

#[cfg(test)]
mod tests {
    use super::super::docx::tests::zip_of;
    use super::*;

    #[test]
    fn converts_slides_in_order_with_titles_tables_and_notes() {
        let slide = |title: &str, body: &str| {
            format!(
                r#"<p:sld xmlns:p="p" xmlns:a="a"><p:cSld><p:spTree>
<p:sp><p:nvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr><p:txBody><a:p><a:r><a:t>{title}</a:t></a:r></a:p></p:txBody></p:sp>
{body}
</p:spTree></p:cSld></p:sld>"#
            )
        };
        let s1 = slide(
            "課題",
            r#"<p:sp><p:txBody><a:p><a:r><a:t>手作業が多い</a:t></a:r></a:p><a:p><a:pPr lvl="1"/><a:r><a:t>月末の集計</a:t></a:r></a:p></p:txBody></p:sp><p:pic/>"#,
        );
        let s2 = slide(
            "比較",
            r#"<p:graphicFrame><a:graphic><a:graphicData><a:tbl><a:tr><a:tc><a:txBody><a:p><a:r><a:t>A</a:t></a:r></a:p></a:txBody></a:tc><a:tc><a:txBody><a:p><a:r><a:t>B</a:t></a:r></a:p></a:txBody></a:tc></a:tr></a:tbl></a:graphicData></a:graphic></p:graphicFrame>"#,
        );
        let notes = r#"<p:notes xmlns:p="p" xmlns:a="a"><p:cSld><p:spTree><p:sp><p:nvSpPr><p:nvPr><p:ph type="body"/></p:nvPr></p:nvSpPr><p:txBody><a:p><a:r><a:t>ここで強調</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:notes>"#;
        let rels = r#"<Relationships><Relationship Id="rId2" Target="../notesSlides/notesSlide1.xml"/></Relationships>"#;
        let bytes = zip_of(&[
            ("ppt/slides/slide10.xml", &s2),
            ("ppt/slides/slide2.xml", &s1),
            ("ppt/slides/_rels/slide2.xml.rels", rels),
            ("ppt/notesSlides/notesSlide1.xml", notes),
        ]);
        let converted = convert(&bytes).unwrap();
        assert_eq!(
            converted.body,
            "## 2. 課題\n\n- 手作業が多い\n  - 月末の集計\n\n> ノート: ここで強調\n\n## 10. 比較\n\n| A | B |\n|---|---|"
        );
        assert_eq!(converted.warnings.len(), 1);
    }
}
