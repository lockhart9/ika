//! データセットの置き場所。定義は登録簿 `data/datasets.csv`（1データセット1行）、
//! 行は `data/datasets/<name>.csv`（ヘッダは列名だけの普通の CSV。並びは追記した順）。

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use super::schema::{Column, Value};
use crate::kb::atomic;

pub const REGISTRY: &str = "data/datasets.csv";
pub const DIR: &str = "data/datasets";
/// 以前の置き場所（SQLite）。残っていれば `ika check` が移し方を案内する
pub const LEGACY: &str = "data/datasets.sqlite3";

/// 登録簿の1行（CSV の形そのまま）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub name: String,
    pub summary: String,
    pub domain: String,
    /// `;` 区切り
    pub projects: String,
    /// `;` 区切り
    pub key: String,
    /// `name:type` を `;` 区切り
    pub columns: String,
    pub created: String,
}

/// データセットの定義
#[derive(Debug, Clone)]
pub struct Dataset {
    pub name: String,
    pub summary: String,
    pub domain: String,
    pub projects: Vec<String>,
    /// 重複判定に使う列。空なら重複を許す
    pub key: Vec<String>,
    pub columns: Vec<Column>,
    pub created: String,
}

fn split(text: &str) -> Vec<String> {
    text.split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

impl Entry {
    pub fn dataset(&self) -> Result<Dataset> {
        let columns = split(&self.columns)
            .iter()
            .map(|c| Column::parse(c))
            .collect::<Result<Vec<_>>>()?;
        if columns.is_empty() {
            bail!("列の定義がありません");
        }
        Ok(Dataset {
            name: self.name.clone(),
            summary: self.summary.clone(),
            domain: self.domain.clone(),
            projects: split(&self.projects),
            key: split(&self.key),
            columns,
            created: self.created.clone(),
        })
    }
}

impl Dataset {
    pub fn entry(&self) -> Entry {
        Entry {
            name: self.name.clone(),
            summary: self.summary.clone(),
            domain: self.domain.clone(),
            projects: self.projects.join(";"),
            key: self.key.join(";"),
            columns: self
                .columns
                .iter()
                .map(Column::spec)
                .collect::<Vec<_>>()
                .join(";"),
            created: self.created.clone(),
        }
    }

    pub fn names(&self) -> Vec<&str> {
        self.columns.iter().map(|c| c.name.as_str()).collect()
    }

    pub fn column(&self, name: &str) -> Result<(usize, &Column)> {
        match self
            .columns
            .iter()
            .enumerate()
            .find(|(_, c)| c.name == name)
        {
            Some(found) => Ok(found),
            None => bail!(
                "定義にない列です: {name}（列: {}）",
                self.names().join(", ")
            ),
        }
    }

    /// key の列の位置。key が無ければ空
    pub fn key_positions(&self) -> Vec<usize> {
        self.key
            .iter()
            .filter_map(|k| self.columns.iter().position(|c| &c.name == k))
            .collect()
    }
}

pub fn rows_rel(name: &str) -> String {
    format!("{DIR}/{name}.csv")
}

pub fn rows_path(root: &Path, name: &str) -> PathBuf {
    root.join(rows_rel(name))
}

/// 登録簿の全行。ファイルが無ければ空
pub fn entries(root: &Path) -> Result<Vec<Entry>> {
    let file = root.join(REGISTRY);
    if !file.is_file() {
        return Ok(Vec::new());
    }
    crate::registry::load(root, REGISTRY)
}

/// 全データセットの定義（名前順）
pub fn list(root: &Path) -> Result<Vec<Dataset>> {
    let mut all = entries(root)?
        .iter()
        .map(|e| {
            e.dataset()
                .with_context(|| format!("{REGISTRY}: {}: 定義が不正です", e.name))
        })
        .collect::<Result<Vec<_>>>()?;
    all.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(all)
}

pub fn find(root: &Path, name: &str) -> Result<Option<Dataset>> {
    Ok(list(root)?.into_iter().find(|d| d.name == name))
}

pub fn get(root: &Path, name: &str) -> Result<Dataset> {
    match find(root, name)? {
        Some(dataset) => Ok(dataset),
        None => bail!("そのデータセットはありません: {name}（`ika data ls` で一覧できます）"),
    }
}

fn registry_text(entries: &[Entry]) -> Result<String> {
    let mut writer = csv::Writer::from_writer(Vec::new());
    if entries.is_empty() {
        writer.write_record([
            "name", "summary", "domain", "projects", "key", "columns", "created",
        ])?;
    }
    for entry in entries {
        writer.serialize(entry)?;
    }
    Ok(String::from_utf8(writer.into_inner()?)?)
}

/// 登録簿を書き直す
pub fn save_entries(root: &Path, entries: &[Entry]) -> Result<()> {
    let file = root.join(REGISTRY);
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir)?;
    }
    atomic::overwrite(&file, &registry_text(entries)?)
}

/// 行（CSV に書く形の文字列）を CSV の本文にする
pub fn rows_text(ds: &Dataset, rows: &[Vec<String>]) -> Result<String> {
    let mut writer = csv::Writer::from_writer(Vec::new());
    writer.write_record(ds.names())?;
    for row in rows {
        writer.write_record(row)?;
    }
    Ok(String::from_utf8(writer.into_inner()?)?)
}

/// 行ファイルの本文を読む。ヘッダが定義の列と違えば失敗する（`ika check` で見つかる）
pub fn parse_rows(ds: &Dataset, text: &str) -> Result<Vec<Vec<String>>> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(false)
        .from_reader(text.as_bytes());
    let header: Vec<String> = reader
        .headers()
        .with_context(|| format!("{} のヘッダを読めません", rows_rel(&ds.name)))?
        .iter()
        .map(str::to_string)
        .collect();
    if header != ds.names() {
        bail!(
            "{} のヘッダ（{}）が定義の列（{}）と違います",
            rows_rel(&ds.name),
            header.join(", "),
            ds.names().join(", ")
        );
    }
    reader
        .records()
        .map(|r| {
            r.map(|r| r.iter().map(str::to_string).collect())
                .with_context(|| format!("{} を読めません", rows_rel(&ds.name)))
        })
        .collect()
}

pub fn read_text(root: &Path, ds: &Dataset) -> Result<String> {
    let path = rows_path(root, &ds.name);
    fs::read_to_string(&path).with_context(|| format!("{} を読めません", rows_rel(&ds.name)))
}

/// 行を文字列のまま読む
pub fn raw_rows(root: &Path, ds: &Dataset) -> Result<Vec<Vec<String>>> {
    parse_rows(ds, &read_text(root, ds)?)
}

/// 行を型どおりの値で読む。型に合わないセルがあれば失敗する
pub fn rows(root: &Path, ds: &Dataset) -> Result<Vec<Vec<Value>>> {
    typed(ds, &raw_rows(root, ds)?)
}

pub fn typed(ds: &Dataset, raw: &[Vec<String>]) -> Result<Vec<Vec<Value>>> {
    raw.iter()
        .enumerate()
        .map(|(i, row)| {
            row.iter()
                .zip(&ds.columns)
                .map(|(cell, col)| {
                    col.parse_cell(cell).with_context(|| {
                        format!(
                            "{}: {} 行目: {}（`ika check` で確かめてください）",
                            rows_rel(&ds.name),
                            i + 2,
                            col.name
                        )
                    })
                })
                .collect()
        })
        .collect()
}

/// 行の数
pub fn row_count(root: &Path, ds: &Dataset) -> Result<usize> {
    Ok(raw_rows(root, ds)?.len())
}

/// key の値（重複判定用）
pub fn key_of(positions: &[usize], row: &[String]) -> Vec<String> {
    positions.iter().map(|&i| row[i].clone()).collect()
}

/// 行ファイルのある名前（`.csv` だけ。一時ファイルは除く）
pub fn row_files(root: &Path) -> Result<HashSet<String>> {
    let dir = root.join(DIR);
    let mut names = HashSet::new();
    if !dir.is_dir() {
        return Ok(names);
    }
    for entry in fs::read_dir(dir)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        if let Some(stem) = name.strip_suffix(".csv") {
            names.insert(stem.to_string());
        }
    }
    Ok(names)
}
