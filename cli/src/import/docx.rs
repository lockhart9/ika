//! Word（.docx）を Markdown にする。見出し・箇条書き・表・リンク・太字を残し、画像は数だけ知らせる

use std::collections::HashMap;

use anyhow::{Result, bail};

use super::convert::{Converted, core_title, open_zip, table, zip_entry};
use super::xml::{self, Element, Node};

/// 文書全体で使う対応表
struct Context {
    /// スタイル ID → 見出しのレベル（1〜6）
    headings: HashMap<String, usize>,
    /// (numId, ilvl) → 番号付きか
    ordered: HashMap<(String, String), bool>,
    /// リレーション ID → リンク先
    links: HashMap<String, String>,
    images: usize,
}

pub fn convert(bytes: &[u8]) -> Result<Converted> {
    let mut archive = open_zip(bytes)?;
    let Some(document) = zip_entry(&mut archive, "word/document.xml")? else {
        bail!("word/document.xml がありません（docx ではないかもしれません）");
    };
    let mut ctx = Context {
        headings: zip_entry(&mut archive, "word/styles.xml")?
            .map(|s| headings(&s))
            .transpose()?
            .unwrap_or_default(),
        ordered: zip_entry(&mut archive, "word/numbering.xml")?
            .map(|s| numbering(&s))
            .transpose()?
            .unwrap_or_default(),
        links: zip_entry(&mut archive, "word/_rels/document.xml.rels")?
            .map(|s| relationships(&s))
            .transpose()?
            .unwrap_or_default(),
        images: 0,
    };
    let title = core_title(&mut archive)?;

    let root = xml::parse(&document)?;
    let Some(body) = root.child("document").and_then(|d| d.child("body")) else {
        bail!("本文（w:body）がありません");
    };
    let mut blocks = Vec::new();
    collect_blocks(body, &mut ctx, &mut blocks);

    let mut warnings = Vec::new();
    if ctx.images > 0 {
        warnings.push(format!(
            "画像 {} 個は取り込んでいません（文字だけを取り込みます）",
            ctx.images
        ));
    }
    Ok(Converted {
        body: join_blocks(&blocks),
        title,
        warnings,
    })
}

/// 本文のブロック。箇条書きは続けて並べ、それ以外は空行で区切る
pub(super) struct Block {
    pub text: String,
    pub list: bool,
}

pub(super) fn join_blocks(blocks: &[Block]) -> String {
    let mut out = String::new();
    for (i, block) in blocks.iter().enumerate() {
        if i > 0 {
            out.push_str(if block.list && blocks[i - 1].list {
                "\n"
            } else {
                "\n\n"
            });
        }
        out.push_str(&block.text);
    }
    out
}

fn collect_blocks(parent: &Element, ctx: &mut Context, out: &mut Vec<Block>) {
    for el in parent.elements() {
        match el.name.as_str() {
            "p" => {
                if let Some(block) = paragraph(el, ctx) {
                    out.push(block);
                }
            }
            "tbl" => {
                let rows = table_rows(el, ctx);
                if !rows.is_empty() {
                    out.push(Block {
                        text: table(&rows),
                        list: false,
                    });
                }
            }
            // 目次などのコンテンツコントロール・変更履歴の挿入の中にも段落がある
            "sdt" | "sdtContent" | "ins" | "customXml" => collect_blocks(el, ctx, out),
            _ => {}
        }
    }
}

fn paragraph(p: &Element, ctx: &mut Context) -> Option<Block> {
    let props = p.child("pPr");
    let level = props.and_then(|pr| {
        pr.child("outlineLvl")
            .and_then(|o| o.attr("val"))
            .and_then(|v| v.parse::<usize>().ok())
            .map(|v| v + 1)
            .or_else(|| {
                pr.child("pStyle")
                    .and_then(|s| s.attr("val"))
                    .and_then(|id| ctx.headings.get(id).copied())
            })
    });
    let heading = level.filter(|l| (1..=6).contains(l));

    let mut runs = Vec::new();
    inline(p, ctx, heading.is_none(), &mut runs);
    let text = merge_runs(&runs);
    let text = text.trim();
    if text.is_empty() {
        return None;
    }

    if let Some(level) = heading {
        return Some(Block {
            text: format!("{} {}", "#".repeat(level), text.replace('\n', " ")),
            list: false,
        });
    }
    if let Some(num) = props.and_then(|pr| pr.child("numPr")) {
        let id = num.child("numId").and_then(|n| n.attr("val")).unwrap_or("");
        // numId 0 は「番号なし」
        if id != "0" {
            let ilvl = num.child("ilvl").and_then(|n| n.attr("val")).unwrap_or("0");
            let depth = ilvl.parse::<usize>().unwrap_or(0);
            let ordered = ctx
                .ordered
                .get(&(id.to_string(), ilvl.to_string()))
                .copied()
                .unwrap_or(false);
            let marker = if ordered { "1." } else { "-" };
            return Some(Block {
                text: format!("{}{marker} {}", "  ".repeat(depth), text.replace('\n', " ")),
                list: true,
            });
        }
    }
    Some(Block {
        text: text.replace('\n', "  \n"),
        list: false,
    })
}

/// 段落の中の文字。(文字, 太字か)
fn inline(el: &Element, ctx: &mut Context, allow_bold: bool, out: &mut Vec<(String, bool)>) {
    for child in el.elements() {
        match child.name.as_str() {
            "r" => {
                let bold = allow_bold && is_bold(child);
                for part in child.elements() {
                    match part.name.as_str() {
                        "t" => out.push((text(part), bold)),
                        "tab" => out.push(("\t".into(), bold)),
                        "br" | "cr" => out.push(("\n".into(), false)),
                        "drawing" | "pict" | "object" => ctx.images += 1,
                        _ => {}
                    }
                }
            }
            "hyperlink" => {
                let mut inner = Vec::new();
                inline(child, ctx, false, &mut inner);
                let label = merge_runs(&inner);
                let target = child.attr("id").and_then(|id| ctx.links.get(id));
                match target {
                    Some(url) if !label.trim().is_empty() => {
                        out.push((format!("[{}]({url})", label.trim()), false))
                    }
                    _ => out.push((label, false)),
                }
            }
            // フィールド・変更履歴の挿入・スマートタグの中の文字（削除は w:delText なので拾わない）
            "fldSimple" | "ins" | "smartTag" | "sdt" | "sdtContent" | "customXml" => {
                inline(child, ctx, allow_bold, out)
            }
            _ => {}
        }
    }
}

fn text(el: &Element) -> String {
    el.children
        .iter()
        .filter_map(|n| match n {
            Node::Text(t) => Some(t.as_str()),
            Node::Element(_) => None,
        })
        .collect()
}

fn is_bold(run: &Element) -> bool {
    run.child("rPr")
        .and_then(|pr| pr.child("b"))
        .is_some_and(|b| !matches!(b.attr("val"), Some("0" | "false" | "off")))
}

/// 隣り合う同じ太さの文字をまとめ、太字を `**` で囲む
fn merge_runs(runs: &[(String, bool)]) -> String {
    let mut merged: Vec<(String, bool)> = Vec::new();
    for (text, bold) in runs {
        match merged.last_mut() {
            Some((last, last_bold)) if last_bold == bold => last.push_str(text),
            _ => merged.push((text.clone(), *bold)),
        }
    }
    merged
        .into_iter()
        .map(|(text, bold)| {
            let trimmed = text.trim();
            if bold && !trimmed.is_empty() {
                // 前後の空白は ** の外に出す（`** 太字**` は太字にならない）
                let start = &text[..text.len() - text.trim_start().len()];
                let end = &text[text.trim_end().len()..];
                format!("{start}**{trimmed}**{end}")
            } else {
                text
            }
        })
        .collect()
}

fn table_rows(tbl: &Element, ctx: &mut Context) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    for tr in tbl.elements().filter(|e| e.name == "tr") {
        let mut row = Vec::new();
        for tc in tr.elements().filter(|e| e.name == "tc") {
            let mut blocks = Vec::new();
            collect_blocks(tc, ctx, &mut blocks);
            let text: Vec<_> = blocks.iter().map(|b| b.text.replace("\n", " ")).collect();
            row.push(text.join("<br>"));
            // 横に結合したセルは、残りの列を空で埋める
            let span = tc
                .child("tcPr")
                .and_then(|pr| pr.child("gridSpan"))
                .and_then(|g| g.attr("val"))
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(1);
            row.extend(std::iter::repeat_n(String::new(), span.saturating_sub(1)));
        }
        rows.push(row);
    }
    rows
}

/// styles.xml から、見出しのスタイル ID とレベル。日本語版の Word ではスタイル ID が "1" のように
/// 名前と無関係なので、w:name（"heading 1"）と w:outlineLvl で判定する
fn headings(styles: &str) -> Result<HashMap<String, usize>> {
    let root = xml::parse(styles)?;
    let mut found = Vec::new();
    root.descendants("style", &mut found);
    let mut map = HashMap::new();
    for style in found {
        let Some(id) = style.attr("styleId") else {
            continue;
        };
        let name = style
            .child("name")
            .and_then(|n| n.attr("val"))
            .unwrap_or("")
            .to_ascii_lowercase();
        let level = if name == "title" {
            Some(1)
        } else if let Some(n) = name.strip_prefix("heading ") {
            n.trim().parse().ok()
        } else {
            style
                .child("pPr")
                .and_then(|pr| pr.child("outlineLvl"))
                .and_then(|o| o.attr("val"))
                .and_then(|v| v.parse::<usize>().ok())
                .map(|v| v + 1)
        };
        if let Some(level) = level {
            map.insert(id.to_string(), level);
        }
    }
    Ok(map)
}

/// numbering.xml から、(numId, ilvl) が番号付きか（bullet 以外）
fn numbering(text: &str) -> Result<HashMap<(String, String), bool>> {
    let root = xml::parse(text)?;
    let Some(numbering) = root.child("numbering") else {
        return Ok(HashMap::new());
    };
    let mut abstracts: HashMap<&str, Vec<(String, bool)>> = HashMap::new();
    for abs in numbering.elements().filter(|e| e.name == "abstractNum") {
        let Some(id) = abs.attr("abstractNumId") else {
            continue;
        };
        let levels = abs
            .elements()
            .filter(|e| e.name == "lvl")
            .map(|lvl| {
                let format = lvl.child("numFmt").and_then(|f| f.attr("val"));
                (
                    lvl.attr("ilvl").unwrap_or("0").to_string(),
                    !matches!(format, Some("bullet" | "none") | None),
                )
            })
            .collect();
        abstracts.insert(id, levels);
    }
    let mut map = HashMap::new();
    for num in numbering.elements().filter(|e| e.name == "num") {
        let (Some(id), Some(abs)) = (
            num.attr("numId"),
            num.child("abstractNumId").and_then(|a| a.attr("val")),
        ) else {
            continue;
        };
        for (ilvl, ordered) in abstracts.get(abs).into_iter().flatten() {
            map.insert((id.to_string(), ilvl.clone()), *ordered);
        }
    }
    Ok(map)
}

/// .rels から、外部リンクのリレーション ID → URL
pub(super) fn relationships(text: &str) -> Result<HashMap<String, String>> {
    let root = xml::parse(text)?;
    let mut found = Vec::new();
    root.descendants("Relationship", &mut found);
    Ok(found
        .into_iter()
        .filter_map(|rel| Some((rel.attr("Id")?.to_string(), rel.attr("Target")?.to_string())))
        .collect())
}

#[cfg(test)]
pub(super) mod tests {
    use std::io::{Cursor, Write};

    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    use super::*;

    /// テスト用に Office 形式の zip を組み立てる
    pub fn zip_of(files: &[(&str, &str)]) -> Vec<u8> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, text) in files {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(text.as_bytes()).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    const W: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;

    #[test]
    fn converts_headings_lists_tables_links_and_bold() {
        let document = format!(
            r#"<w:document {W}><w:body>
<w:p><w:pPr><w:pStyle w:val="1"/></w:pPr><w:r><w:t>概要</w:t></w:r></w:p>
<w:p><w:r><w:t xml:space="preserve">普通の </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>太字</w:t></w:r><w:r><w:t>と</w:t></w:r><w:hyperlink r:id="rId9"><w:r><w:t>リンク</w:t></w:r></w:hyperlink></w:p>
<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>一つ目</w:t></w:r></w:p>
<w:p><w:pPr><w:numPr><w:ilvl w:val="1"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>入れ子</w:t></w:r></w:p>
<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="2"/></w:numPr></w:pPr><w:r><w:t>手順</w:t></w:r></w:p>
<w:p/>
<w:tbl><w:tr><w:tc><w:p><w:r><w:t>項目</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>値</w:t></w:r></w:p></w:tc></w:tr>
<w:tr><w:tc><w:tcPr><w:gridSpan w:val="2"/></w:tcPr><w:p><w:r><w:t>結合</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
<w:p><w:r><w:drawing/></w:r></w:p>
</w:body></w:document>"#
        );
        let styles = format!(
            r#"<w:styles {W}><w:style w:styleId="1"><w:name w:val="heading 1"/></w:style></w:styles>"#
        );
        let numbering = format!(
            r#"<w:numbering {W}>
<w:abstractNum w:abstractNumId="10"><w:lvl w:ilvl="0"><w:numFmt w:val="bullet"/></w:lvl><w:lvl w:ilvl="1"><w:numFmt w:val="bullet"/></w:lvl></w:abstractNum>
<w:abstractNum w:abstractNumId="20"><w:lvl w:ilvl="0"><w:numFmt w:val="decimal"/></w:lvl></w:abstractNum>
<w:num w:numId="1"><w:abstractNumId w:val="10"/></w:num><w:num w:numId="2"><w:abstractNumId w:val="20"/></w:num>
</w:numbering>"#
        );
        let rels = r#"<Relationships><Relationship Id="rId9" Target="https://example.com/x" TargetMode="External"/></Relationships>"#;
        let core = r#"<cp:coreProperties xmlns:cp="c" xmlns:dc="d"><dc:title>製品仕様書</dc:title></cp:coreProperties>"#;
        let bytes = zip_of(&[
            ("word/document.xml", &document),
            ("word/styles.xml", &styles),
            ("word/numbering.xml", &numbering),
            ("word/_rels/document.xml.rels", rels),
            ("docProps/core.xml", core),
        ]);
        let converted = convert(&bytes).unwrap();
        assert_eq!(converted.title.as_deref(), Some("製品仕様書"));
        assert_eq!(
            converted.body,
            "# 概要\n\n普通の **太字**と[リンク](https://example.com/x)\n\n- 一つ目\n  - 入れ子\n1. 手順\n\n| 項目 | 値 |\n|---|---|\n| 結合 |  |"
        );
        assert_eq!(converted.warnings.len(), 1, "画像の知らせ");
    }
}
