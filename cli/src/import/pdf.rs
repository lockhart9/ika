//! PDF の文字を取り出す。ページの区切りに `<!-- page N -->` を入れる（引用するときにページを示せるように）。
//! 見出しや表の構造は PDF に残っていないので、文字の並びだけになる

use std::panic;

use anyhow::{Result, anyhow, bail};

use super::convert::Converted;

pub fn convert(bytes: &[u8]) -> Result<Converted> {
    let pages = extract(bytes)?;
    let mut sections = Vec::new();
    for (i, page) in pages.iter().enumerate() {
        let text = page.trim();
        if !text.is_empty() {
            sections.push(format!("<!-- page {} -->\n\n{text}", i + 1));
        }
    }
    if sections.is_empty() {
        bail!(
            "PDF から文字を取り出せません（スキャン画像の PDF かもしれません）。\
             AI に PDF を読ませて Markdown にし、`ika import - --format md --source <原本>` で渡してください"
        );
    }
    Ok(Converted {
        body: sections.join("\n\n"),
        title: None,
        warnings: Vec::new(),
    })
}

/// pdf-extract は壊れた・特殊な PDF で panic することがあるので、エラーとして受け止める
fn extract(bytes: &[u8]) -> Result<Vec<String>> {
    let hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let result = panic::catch_unwind(|| pdf_extract::extract_text_from_mem_by_pages(bytes));
    panic::set_hook(hook);
    match result {
        Ok(Ok(pages)) => Ok(pages),
        Ok(Err(err)) => Err(anyhow!("PDF を読めません: {err}")),
        Err(_) => Err(anyhow!(
            "PDF を読めません（この PDF の形式に対応していません）"
        )),
    }
}

#[cfg(test)]
mod tests {
    use lopdf::content::{Content, Operation};
    use lopdf::{Document, Object, Stream, dictionary};

    use super::*;

    /// 1ページに1行ずつ文字を置いた PDF
    fn pdf_of(pages: &[&str]) -> Vec<u8> {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        let resources_id = doc.add_object(dictionary! {
            "Font" => dictionary! { "F1" => font_id },
        });
        let mut kids = Vec::new();
        for text in pages {
            let content = Content {
                operations: vec![
                    Operation::new("BT", vec![]),
                    Operation::new("Tf", vec!["F1".into(), 24.into()]),
                    Operation::new("Td", vec![100.into(), 600.into()]),
                    Operation::new("Tj", vec![Object::string_literal(*text)]),
                    Operation::new("ET", vec![]),
                ],
            };
            let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
            let page_id = doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => content_id,
            });
            kids.push(page_id.into());
        }
        let count = kids.len() as i64;
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => kids,
                "Count" => count,
                "Resources" => resources_id,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    }

    #[test]
    fn extracts_text_with_page_markers() {
        let converted = convert(&pdf_of(&["Pricing plans", "Free tier"])).unwrap();
        assert!(
            converted.body.starts_with("<!-- page 1 -->\n\n"),
            "{}",
            converted.body
        );
        assert!(converted.body.contains("Pricing plans"));
        assert!(converted.body.contains("<!-- page 2 -->\n\nFree tier"));
    }

    #[test]
    fn broken_pdf_is_an_error_not_a_panic() {
        assert!(convert(b"%PDF-1.5 broken").is_err());
    }
}
