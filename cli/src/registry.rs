//! data/ 配下の登録簿（プロダクト・領域）

use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

pub const PROJECTS: &str = "data/projects.csv";
pub const DOMAINS: &str = "data/domains.csv";
pub const STATUSES: &[&str] = &["active", "paused", "archived"];
/// 領域ごとに置くノートの種類（ディレクトリ名）
pub const KIND_DIRS: &[&str] = &["notes", "insights", "decisions"];
/// kb 直下で領域名に使えない名前
pub const RESERVED: &[&str] = &["projects"];

/// data/projects.csv の1行
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub path: String,
    pub url: String,
    pub status: String,
    pub summary: String,
}

/// data/domains.csv の1行。kb/<name>/ にその領域のナレッジを置く
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Domain {
    pub name: String,
    pub summary: String,
}

pub fn projects(root: &Path) -> Result<Vec<Project>> {
    load(root, PROJECTS)
}

pub fn domains(root: &Path) -> Result<Vec<Domain>> {
    load(root, DOMAINS)
}

fn load<T: DeserializeOwned>(root: &Path, file: &str) -> Result<Vec<T>> {
    let mut reader =
        csv::Reader::from_path(root.join(file)).with_context(|| format!("{file} を開けません"))?;
    reader
        .deserialize()
        .collect::<Result<Vec<T>, _>>()
        .with_context(|| format!("{file} の形式が不正です"))
}

pub fn append<T: Serialize>(root: &Path, file: &str, row: &T) -> Result<()> {
    let file = OpenOptions::new().append(true).open(root.join(file))?;
    let mut writer = csv::WriterBuilder::new()
        .has_headers(false)
        .from_writer(file);
    writer.serialize(row)?;
    writer.flush()?;
    Ok(())
}

/// 登録簿を全行書き直す（既存行の編集用）
pub fn save<T: Serialize>(root: &Path, file: &str, rows: &[T]) -> Result<()> {
    let mut writer = csv::Writer::from_path(root.join(file))?;
    for row in rows {
        writer.serialize(row)?;
    }
    writer.flush()?;
    Ok(())
}

/// 登録名は英小文字・数字・ハイフンのみ（ディレクトリ名とフロントマターで使うため）
pub fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && !name.ends_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

pub fn readme_path(root: &Path, name: &str) -> PathBuf {
    root.join("kb/projects").join(name).join("README.md")
}
