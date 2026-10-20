//! 表計算（xlsx・xls・ods）と CSV を Markdown の表にする。シートごとに見出しを立てる

use std::io::Cursor;

use anyhow::{Context, Result, bail};
use calamine::{Data, DataType, Reader, open_workbook_auto_from_rs};

use super::convert::{Converted, table};

/// これより行が多いシートは、データセット（`ika data`）に入れることを勧める
const MANY_ROWS: usize = 200;

pub fn workbook(bytes: &[u8]) -> Result<Converted> {
    let mut book = open_workbook_auto_from_rs(Cursor::new(bytes))
        .context("表計算のファイルとして開けません")?;
    let mut sections = Vec::new();
    let mut warnings = Vec::new();
    for name in book.sheet_names() {
        let range = book
            .worksheet_range(&name)
            .with_context(|| format!("シート {name} を読めません"))?;
        let rows: Vec<Vec<String>> = range
            .rows()
            .map(|row| row.iter().map(cell).collect())
            .collect();
        let rows = trim(rows);
        if rows.is_empty() {
            continue;
        }
        if rows.len() > MANY_ROWS {
            warnings.push(format!(
                "シート {name} は {} 行あります。行が増え続ける観測なら `ika data` のデータセットに入れることを検討してください",
                rows.len()
            ));
        }
        sections.push(format!("## {name}\n\n{}", table(&rows)));
    }
    if sections.is_empty() {
        bail!("中身のあるシートがありません");
    }
    Ok(Converted {
        body: sections.join("\n\n"),
        title: None,
        warnings,
    })
}

pub fn csv(text: &str) -> Result<String> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(text.as_bytes());
    let mut rows = Vec::new();
    for record in reader.records() {
        rows.push(record?.iter().map(str::to_string).collect());
    }
    Ok(table(&trim(rows)))
}

fn cell(data: &Data) -> String {
    match data {
        Data::Empty => String::new(),
        Data::String(s) | Data::DateTimeIso(s) | Data::DurationIso(s) => s.clone(),
        Data::Int(n) => n.to_string(),
        Data::Float(f) => {
            if f.fract() == 0.0 && f.abs() < 1e15 {
                format!("{}", *f as i64)
            } else {
                f.to_string()
            }
        }
        Data::Bool(b) => b.to_string(),
        Data::DateTime(_) => match data.as_datetime() {
            Some(dt) if dt.time() == chrono::NaiveTime::MIN => dt.date().to_string(),
            Some(dt) => dt.format("%Y-%m-%d %H:%M:%S").to_string(),
            None => data.to_string(),
        },
        Data::Error(e) => format!("#{e:?}"),
    }
}

/// 上下の空行と、右端の空の列を落とす
fn trim(mut rows: Vec<Vec<String>>) -> Vec<Vec<String>> {
    let empty = |row: &Vec<String>| row.iter().all(|c| c.trim().is_empty());
    while rows.last().is_some_and(empty) {
        rows.pop();
    }
    let start = rows.iter().position(|r| !empty(r)).unwrap_or(rows.len());
    rows.drain(..start);
    let width = rows
        .iter()
        .map(|r| {
            r.iter()
                .rposition(|c| !c.trim().is_empty())
                .map_or(0, |i| i + 1)
        })
        .max()
        .unwrap_or(0);
    for row in &mut rows {
        row.truncate(width);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workbook_renders_each_sheet_as_a_table() {
        let mut book = rust_xlsxwriter::Workbook::new();
        let sheet = book.add_worksheet().set_name("料金").unwrap();
        sheet.write(1, 0, "プラン").unwrap();
        sheet.write(1, 1, "月額").unwrap();
        sheet.write(2, 0, "Free").unwrap();
        sheet.write(2, 1, 0).unwrap();
        sheet.write(3, 0, "Pro").unwrap();
        sheet.write(3, 1, 980.5).unwrap();
        book.add_worksheet().set_name("空").unwrap();
        let bytes = book.save_to_buffer().unwrap();

        let converted = workbook(&bytes).unwrap();
        assert_eq!(
            converted.body,
            "## 料金\n\n| プラン | 月額 |\n|---|---|\n| Free | 0 |\n| Pro | 980.5 |"
        );
    }

    #[test]
    fn csv_becomes_a_table() {
        assert_eq!(
            csv("a,b\n1,\"x,y\"\n\n").unwrap(),
            "| a | b |\n|---|---|\n| 1 | x,y |"
        );
    }
}
