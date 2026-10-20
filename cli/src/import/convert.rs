//! 原本のバイト列を Markdown にする。外部コマンドは使わず、Rust のライブラリだけで変換する

use std::io::{Cursor, Read};

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use zip::ZipArchive;

use super::{docx, pdf, pptx, sheet, xml};

#[derive(Clone, Copy, PartialEq, Eq, Debug, ValueEnum)]
pub enum Format {
    Md,
    Txt,
    Html,
    Csv,
    Docx,
    Xlsx,
    Xls,
    Ods,
    Pptx,
    Pdf,
}

impl Format {
    /// 拡張子から決める
    pub fn from_extension(ext: &str) -> Option<Format> {
        Some(match ext.to_ascii_lowercase().as_str() {
            "md" | "markdown" => Format::Md,
            "txt" | "text" => Format::Txt,
            "html" | "htm" => Format::Html,
            "csv" => Format::Csv,
            "docx" => Format::Docx,
            "xlsx" | "xlsm" => Format::Xlsx,
            "xls" => Format::Xls,
            "ods" => Format::Ods,
            "pptx" => Format::Pptx,
            "pdf" => Format::Pdf,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Format::Md => "md",
            Format::Txt => "txt",
            Format::Html => "html",
            Format::Csv => "csv",
            Format::Docx => "docx",
            Format::Xlsx => "xlsx",
            Format::Xls => "xls",
            Format::Ods => "ods",
            Format::Pptx => "pptx",
            Format::Pdf => "pdf",
        }
    }
}

/// 変換の結果
pub struct Converted {
    /// 本文（Markdown）
    pub body: String,
    /// 原本に書かれていたタイトル（docx・pptx の文書プロパティ、Markdown の最初の `# 見出し`）
    pub title: Option<String>,
    /// 取り込めなかったもの（画像など）の知らせ
    pub warnings: Vec<String>,
}

impl Converted {
    fn text(body: String) -> Converted {
        Converted {
            body,
            title: None,
            warnings: Vec::new(),
        }
    }
}

pub fn convert(bytes: &[u8], format: Format) -> Result<Converted> {
    let text = || {
        String::from_utf8_lossy(bytes)
            .trim_start_matches('\u{feff}')
            .to_string()
    };
    let mut converted = match format {
        Format::Md => {
            let text = text();
            // 原本にフロントマターがあれば外す（ika のフロントマターと二重にしない）
            let body = crate::frontmatter::body(&text).to_string();
            let title = body
                .lines()
                .find_map(|line| line.strip_prefix("# "))
                .map(|t| t.trim().to_string());
            Converted {
                body,
                title,
                warnings: Vec::new(),
            }
        }
        Format::Txt => Converted::text(text()),
        Format::Html => html(&text())?,
        Format::Csv => Converted::text(sheet::csv(&text())?),
        Format::Xlsx | Format::Xls | Format::Ods => sheet::workbook(bytes)?,
        Format::Docx => docx::convert(bytes)?,
        Format::Pptx => pptx::convert(bytes)?,
        Format::Pdf => pdf::convert(bytes)?,
    };
    converted.body = tidy(&converted.body);
    if converted.body.is_empty() {
        bail!("本文が空です");
    }
    Ok(converted)
}

/// HTML を Markdown にする。script・style・ナビゲーションなど本文でないものは落とす
fn html(text: &str) -> Result<Converted> {
    let converter = htmd::HtmlToMarkdown::builder()
        .skip_tags(vec![
            "script", "style", "noscript", "template", "head", "nav", "iframe", "svg",
        ])
        .build();
    let body = converter
        .convert(text)
        .context("HTML を Markdown にできません")?;
    let lower = text.to_lowercase();
    let title = lower.find("<title").and_then(|start| {
        let open = start + lower[start..].find('>')? + 1;
        let close = open + lower[open..].find("</title>")?;
        let title = htmd::convert(&text[open..close]).ok()?;
        Some(title.trim().to_string()).filter(|t| !t.is_empty())
    });
    Ok(Converted {
        body,
        title,
        warnings: Vec::new(),
    })
}

/// 行末の空白と、3行以上続く空行を詰める
fn tidy(text: &str) -> String {
    let mut out = String::new();
    let mut blank = 0;
    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.trim().to_string() + "\n"
}

/// 表を Markdown の表にする。1行目を見出しにし、列数は最も長い行にそろえる
pub fn table(rows: &[Vec<String>]) -> String {
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    if width == 0 {
        return String::new();
    }
    let cell = |text: &str| {
        text.trim()
            .replace('|', "\\|")
            .replace("\r\n", "<br>")
            .replace('\n', "<br>")
    };
    let line = |row: &[String]| {
        let cells: Vec<String> = (0..width)
            .map(|i| cell(row.get(i).map(String::as_str).unwrap_or("")))
            .collect();
        format!("| {} |", cells.join(" | "))
    };
    let mut out = vec![line(&rows[0]), format!("|{}", "---|".repeat(width))];
    out.extend(rows[1..].iter().map(|row| line(row)));
    out.join("\n")
}

/// zip（Office Open XML）の中の1ファイルを文字列で読む。無ければ None
pub fn zip_entry(archive: &mut ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<Option<String>> {
    let mut file = match archive.by_name(name) {
        Ok(file) => file,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(err) => return Err(err).with_context(|| format!("{name} を読めません")),
    };
    let mut text = String::new();
    file.read_to_string(&mut text)
        .with_context(|| format!("{name} を読めません"))?;
    Ok(Some(text))
}

pub fn open_zip(bytes: &[u8]) -> Result<ZipArchive<Cursor<&[u8]>>> {
    ZipArchive::new(Cursor::new(bytes)).context("Office 形式（zip）として開けません")
}

/// docProps/core.xml の dc:title
pub fn core_title(archive: &mut ZipArchive<Cursor<&[u8]>>) -> Result<Option<String>> {
    let Some(core) = zip_entry(archive, "docProps/core.xml")? else {
        return Ok(None);
    };
    let root = xml::parse(&core)?;
    let mut titles = Vec::new();
    root.descendants("title", &mut titles);
    Ok(titles
        .first()
        .map(|el| text_of(el).trim().to_string())
        .filter(|t| !t.is_empty()))
}

/// 要素の中のテキストをすべてつなげる
pub fn text_of(el: &xml::Element) -> String {
    let mut out = String::new();
    for node in &el.children {
        match node {
            xml::Node::Text(text) => out.push_str(text),
            xml::Node::Element(child) => out.push_str(&text_of(child)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_pads_rows_and_escapes_pipes() {
        let rows = vec![
            vec!["a".to_string(), "b".to_string()],
            vec!["x|y".to_string()],
            vec!["1".to_string(), "2\n3".to_string(), "extra".to_string()],
        ];
        assert_eq!(
            table(&rows),
            "| a | b |  |\n|---|---|---|\n| x\\|y |  |  |\n| 1 | 2<br>3 | extra |"
        );
    }

    #[test]
    fn html_keeps_structure_and_drops_scripts() {
        let html = "<html><head><title>料金 &amp; プラン</title><style>p{}</style></head><body>\
            <nav><a href=\"/\">ホーム</a></nav><h1>料金</h1><p>月額 <b>980円</b></p>\
            <ul><li>無料枠あり</li></ul><script>alert(1)</script></body></html>";
        let converted = convert(html.as_bytes(), Format::Html).unwrap();
        assert_eq!(converted.title.as_deref(), Some("料金 & プラン"));
        assert!(
            converted.body.starts_with("# 料金\n\n月額 **980円**\n\n"),
            "{}",
            converted.body
        );
        assert!(converted.body.contains("無料枠あり"));
        assert!(!converted.body.contains("alert") && !converted.body.contains("ホーム"));
    }

    #[test]
    fn markdown_drops_its_own_frontmatter_and_finds_the_title() {
        let md = "---\ntitle: x\n---\n\n# 仕様書\n\n本文\n\n\n\n続き  \n";
        let converted = convert(md.as_bytes(), Format::Md).unwrap();
        assert_eq!(converted.title.as_deref(), Some("仕様書"));
        assert_eq!(converted.body, "# 仕様書\n\n本文\n\n続き\n");
    }
}
