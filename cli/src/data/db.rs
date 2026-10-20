//! `data/datasets.sqlite3`: 全データセットを1ファイルに持つ。
//! データセットごとに1テーブル、定義は `_datasets` に JSON で持つ。
//! git で1ファイルとして扱えるよう、ジャーナルは WAL にしない（既定の DELETE のまま）。

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags, OptionalExtension};

use super::schema::{Column, quote};

pub const FILE: &str = "data/datasets.sqlite3";

const META: &str = "CREATE TABLE IF NOT EXISTS _datasets (
    name TEXT PRIMARY KEY,
    summary TEXT NOT NULL,
    domain TEXT NOT NULL,
    projects TEXT NOT NULL,
    key TEXT NOT NULL,
    columns TEXT NOT NULL,
    created TEXT NOT NULL
)";

/// `_datasets` の1行
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

pub fn path(root: &Path) -> PathBuf {
    root.join(FILE)
}

fn busy(conn: &Connection) -> Result<()> {
    conn.busy_timeout(Duration::from_secs(5))?;
    Ok(())
}

/// 書き込み用に開く。`create` なら、無いときにファイルと `_datasets` を作る
pub fn open_rw(root: &Path, create: bool) -> Result<Connection> {
    let file = path(root);
    if !create && !file.is_file() {
        bail!("データセットがありません（`ika data new` で作ってください）");
    }
    let conn = Connection::open(&file).with_context(|| format!("{FILE} を開けません"))?;
    busy(&conn)?;
    conn.execute_batch(META)?;
    Ok(conn)
}

/// 読み取り専用で開く。ファイルが無ければ `None`
pub fn open_ro(root: &Path) -> Result<Option<Connection>> {
    let file = path(root);
    if !file.is_file() {
        return Ok(None);
    }
    let conn = Connection::open_with_flags(
        &file,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("{FILE} を開けません"))?;
    busy(&conn)?;
    conn.pragma_update(None, "query_only", true)?;
    Ok(Some(conn))
}

fn json_column<T: serde::de::DeserializeOwned>(
    row: &rusqlite::Row,
    idx: usize,
) -> rusqlite::Result<T> {
    let text: String = row.get(idx)?;
    serde_json::from_str(&text).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, e.into())
    })
}

fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Dataset> {
    Ok(Dataset {
        name: row.get(0)?,
        summary: row.get(1)?,
        domain: row.get(2)?,
        projects: json_column(row, 3)?,
        key: json_column(row, 4)?,
        columns: json_column(row, 5)?,
        created: row.get(6)?,
    })
}

const SELECT: &str = "SELECT name, summary, domain, projects, key, columns, created FROM _datasets";

pub fn list(conn: &Connection) -> Result<Vec<Dataset>> {
    let mut stmt = conn.prepare(&format!("{SELECT} ORDER BY name"))?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn find(conn: &Connection, name: &str) -> Result<Option<Dataset>> {
    Ok(conn
        .query_row(&format!("{SELECT} WHERE name = ?1"), [name], from_row)
        .optional()?)
}

pub fn get(conn: &Connection, name: &str) -> Result<Dataset> {
    match find(conn, name)? {
        Some(dataset) => Ok(dataset),
        None => bail!("そのデータセットはありません: {name}（`ika data ls` で一覧できます）"),
    }
}

pub fn insert_meta(conn: &Connection, dataset: &Dataset) -> Result<()> {
    conn.execute(
        "INSERT INTO _datasets (name, summary, domain, projects, key, columns, created)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        (
            &dataset.name,
            &dataset.summary,
            &dataset.domain,
            serde_json::to_string(&dataset.projects)?,
            serde_json::to_string(&dataset.key)?,
            serde_json::to_string(&dataset.columns)?,
            &dataset.created,
        ),
    )?;
    Ok(())
}

/// CREATE TABLE 文
pub fn create_table_sql(dataset: &Dataset) -> String {
    let mut parts: Vec<String> = dataset.columns.iter().map(Column::ddl).collect();
    if !dataset.key.is_empty() {
        let key: Vec<String> = dataset.key.iter().map(|k| quote(k)).collect();
        parts.push(format!("UNIQUE ({})", key.join(", ")));
    }
    format!(
        "CREATE TABLE {} ({})",
        quote(&dataset.name),
        parts.join(", ")
    )
}

pub fn row_count(conn: &Connection, name: &str) -> Result<i64> {
    Ok(
        conn.query_row(&format!("SELECT COUNT(*) FROM {}", quote(name)), [], |r| {
            r.get(0)
        })?,
    )
}

/// 実際のテーブルの列名（定義との突き合わせ用）
pub fn table_columns(conn: &Connection, name: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", quote(name)))?;
    let names = stmt.query_map([], |r| r.get::<_, String>(1))?;
    Ok(names.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// `_datasets` に無いテーブル（手で作られたもの）
pub fn unregistered_tables(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type = 'table'
           AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\' AND name NOT LIKE '\\_%' ESCAPE '\\'
           AND name NOT IN (SELECT name FROM _datasets) ORDER BY name",
    )?;
    let names = stmt.query_map([], |r| r.get::<_, String>(0))?;
    Ok(names.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// 表示用。タブと改行は空白にして、1行1レコードを保つ
pub fn render(value: ValueRef) -> String {
    match value {
        ValueRef::Null => String::new(),
        ValueRef::Integer(n) => n.to_string(),
        ValueRef::Real(x) => x.to_string(),
        ValueRef::Text(bytes) => String::from_utf8_lossy(bytes).replace(['\t', '\n', '\r'], " "),
        ValueRef::Blob(bytes) => format!("<blob {} bytes>", bytes.len()),
    }
}

/// 全行を文字列の表にする。`sql` は SELECT 文
pub fn fetch(conn: &Connection, sql: &str) -> Result<(Vec<String>, Vec<Vec<String>>)> {
    let mut stmt = conn.prepare(sql)?;
    let header: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let count = header.len();
    let mut rows = stmt.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(
            (0..count)
                .map(|i| Ok(render(row.get_ref(i)?)))
                .collect::<Result<Vec<_>>>()?,
        );
    }
    Ok((header, out))
}
