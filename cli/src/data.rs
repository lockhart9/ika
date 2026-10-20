//! `ika data`: 表形式データ（価格の履歴・計測値・ログなど）を SQLite に溜める。
//! 人が直接読むものではないので、読み書きは `ika data` か `ika serve` を通す。

use std::collections::HashSet;
use std::io::{self, Read, Write};
use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::{Subcommand, ValueEnum};
use rusqlite::types::{Value, ValueRef};
use serde_json::json;

use crate::registry;
use crate::vcs;

pub mod db;
pub mod schema;

use db::Dataset;
use schema::{Column, is_ident, quote};

#[derive(Subcommand)]
pub enum Command {
    /// データセット（表）を作る
    New {
        /// 英小文字で始まり、英小文字・数字・アンダースコアだけ
        name: String,
        /// 何のデータか
        #[arg(long)]
        summary: String,
        /// 列。`名前:型`（型は string / int / float / date / bool / enum(a|b)）。省略可の列は型の後ろに `?`。繰り返し指定する
        #[arg(long = "col", required = true)]
        cols: Vec<String>,
        /// 重複判定に使う列（カンマ区切り、必須の列だけ）。同じキーの行は入らない
        #[arg(long, value_delimiter = ',')]
        key: Vec<String>,
        /// 関係する領域（登録済みのもの）
        #[arg(long, default_value = "")]
        domain: String,
        /// 関係するプロダクト（カンマ区切り、登録済みのもの）
        #[arg(long = "project", value_delimiter = ',')]
        projects: Vec<String>,
    },
    /// データセットを一覧する
    Ls,
    /// 定義・行数・末尾の数行を見る
    Show {
        name: String,
        /// 末尾から何行見せるか
        #[arg(long, default_value_t = 5)]
        tail: usize,
    },
    /// 標準入力の CSV（ヘッダ行あり）を検証して追記する。1行でも不正なら何も書かない
    Append {
        name: String,
        /// キーが重複する行の扱い（キーがあるデータセットのみ）
        #[arg(long, value_enum, default_value_t = OnConflict::Error)]
        on_conflict: OnConflict,
    },
    /// 読み取り専用で SELECT を実行する（テーブル名はデータセット名）
    Sql {
        query: String,
        /// JSON の配列で出力する（既定はタブ区切り）
        #[arg(long)]
        json: bool,
    },
    /// CSV として標準出力へ書き出す（移行・共有用）
    Export { name: String },
    /// 条件に合う行を消す（`--where` は SQL の条件式）。間違った行の訂正は、消して追記し直す
    Delete {
        name: String,
        #[arg(long = "where")]
        condition: String,
    },
    /// データセットを丸ごと消す
    Rm { name: String },
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OnConflict {
    /// 重複があれば全体を取り消す
    Error,
    /// 重複する行は入れない
    Skip,
    /// 重複する行は新しい内容で置き換える
    Replace,
}

pub fn run(root: &Path, command: Command) -> Result<()> {
    match command {
        Command::New {
            name,
            summary,
            cols,
            key,
            domain,
            projects,
        } => new(root, name, summary, &cols, key, domain, projects),
        Command::Ls => ls(root),
        Command::Show { name, tail } => show(root, &name, tail),
        Command::Append { name, on_conflict } => append(root, &name, on_conflict),
        Command::Sql { query, json } => sql(root, &query, json),
        Command::Export { name } => export(root, &name),
        Command::Delete { name, condition } => delete(root, &name, &condition),
        Command::Rm { name } => rm(root, &name),
    }
}

fn new(
    root: &Path,
    name: String,
    summary: String,
    cols: &[String],
    key: Vec<String>,
    domain: String,
    projects: Vec<String>,
) -> Result<()> {
    if !is_ident(&name) {
        bail!(
            "データセット名は英小文字で始まり、英小文字・数字・アンダースコアだけ（48 文字まで）にしてください: {name}"
        );
    }
    let columns = cols
        .iter()
        .map(|c| Column::parse(c))
        .collect::<Result<Vec<_>>>()?;
    let mut seen = HashSet::new();
    for col in &columns {
        if !seen.insert(col.name.as_str()) {
            bail!("列名が重複しています: {}", col.name);
        }
    }
    for k in &key {
        match columns.iter().find(|c| &c.name == k) {
            None => bail!("--key の列が定義にありません: {k}"),
            Some(c) if !c.required => {
                bail!("--key には必須の列だけ使えます（`?` を外してください）: {k}")
            }
            Some(_) => {}
        }
    }
    if !domain.is_empty() && !registry::domains(root)?.iter().any(|d| d.name == domain) {
        bail!("未登録の領域です（`ika domain add` で登録してください）: {domain}");
    }
    let registered = registry::projects(root)?;
    for p in &projects {
        if !registered.iter().any(|r| &r.name == p) {
            bail!("未登録のプロダクトです（`ika project add` で登録してください）: {p}");
        }
    }

    let dataset = Dataset {
        name,
        summary,
        domain,
        projects,
        key,
        columns,
        created: chrono::Local::now().format("%Y-%m-%d").to_string(),
    };
    let mut conn = db::open_rw(root, true)?;
    if db::find(&conn, &dataset.name)?.is_some() {
        bail!("すでにあります: {}", dataset.name);
    }
    let tx = conn.transaction()?;
    tx.execute(&db::create_table_sql(&dataset), [])?;
    db::insert_meta(&tx, &dataset)?;
    tx.commit()?;
    drop(conn);

    vcs::commit(root, &format!("data new {}", dataset.name));
    println!("作りました: {}", dataset.name);
    for col in &dataset.columns {
        println!("  {}", col.spec());
    }
    if !dataset.key.is_empty() {
        println!("  key: {}", dataset.key.join(", "));
    }
    Ok(())
}

fn ls(root: &Path) -> Result<()> {
    let Some(conn) = db::open_ro(root)? else {
        return Ok(());
    };
    for ds in db::list(&conn)? {
        println!(
            "{:<24} {:>7} 行  {}",
            ds.name,
            db::row_count(&conn, &ds.name)?,
            ds.summary
        );
    }
    Ok(())
}

fn open_dataset(root: &Path, name: &str) -> Result<(rusqlite::Connection, Dataset)> {
    let Some(conn) = db::open_ro(root)? else {
        bail!("データセットがありません（`ika data new` で作ってください）");
    };
    let ds = db::get(&conn, name)?;
    Ok((conn, ds))
}

fn show(root: &Path, name: &str, tail: usize) -> Result<()> {
    let (conn, ds) = open_dataset(root, name)?;
    println!("{}  {}", ds.name, ds.summary);
    println!("作成: {}", ds.created);
    if !ds.domain.is_empty() {
        println!("領域: {}", ds.domain);
    }
    if !ds.projects.is_empty() {
        println!("プロダクト: {}", ds.projects.join(", "));
    }
    println!("行数: {}", db::row_count(&conn, &ds.name)?);
    if !ds.key.is_empty() {
        println!("key: {}", ds.key.join(", "));
    }
    println!("列:");
    for col in &ds.columns {
        println!("  {}", col.spec());
    }
    if tail > 0 {
        let order = format!(
            "SELECT * FROM (SELECT rowid AS _rowid, * FROM {} ORDER BY rowid DESC LIMIT {tail}) ORDER BY _rowid",
            quote(&ds.name)
        );
        let (header, rows) = db::fetch(&conn, &order)?;
        if !rows.is_empty() {
            println!("末尾 {} 行:", rows.len());
            println!("  {}", header[1..].join("\t"));
            for row in rows {
                println!("  {}", row[1..].join("\t"));
            }
        }
    }
    Ok(())
}

fn read_stdin() -> Result<String> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .context("標準入力を読めません")?;
    let input = input.strip_prefix('\u{feff}').unwrap_or(&input).to_string();
    if input.trim().is_empty() {
        bail!("標準入力が空です（ヘッダ行つきの CSV を渡してください）");
    }
    Ok(input)
}

/// 最大これだけの行エラーを並べて、あとは省略する
const MAX_REPORTED: usize = 10;

fn append(root: &Path, name: &str, on_conflict: OnConflict) -> Result<()> {
    let input = read_stdin()?;
    let mut conn = db::open_rw(root, false)?;
    let ds = db::get(&conn, name)?;
    if on_conflict != OnConflict::Error && ds.key.is_empty() {
        bail!("--on-conflict は key があるデータセットだけで使えます（重複の判定がありません）");
    }

    let mut reader = csv::Reader::from_reader(input.as_bytes());
    let headers = reader.headers().context("ヘッダ行を読めません")?.clone();
    // CSV の列 -> 定義の列の位置
    let mut position = Vec::new();
    for header in &headers {
        let header = header.trim();
        let Some(idx) = ds.columns.iter().position(|c| c.name == header) else {
            bail!("定義にない列です: {header}（列: {}）", names(&ds));
        };
        if position.contains(&idx) {
            bail!("列が重複しています: {header}");
        }
        position.push(idx);
    }
    for (idx, col) in ds.columns.iter().enumerate() {
        if col.required && !position.contains(&idx) {
            bail!("必須の列がありません: {}", col.name);
        }
    }

    let mut rows: Vec<Vec<Value>> = Vec::new();
    let mut errors = Vec::new();
    for record in reader.records() {
        let record = record.context("CSV を読めません")?;
        let line = record.position().map_or(0, |p| p.line());
        let mut values = vec![Value::Null; ds.columns.len()];
        let mut bad = false;
        for (cell, &idx) in record.iter().zip(&position) {
            match ds.columns[idx].parse_cell(cell) {
                Ok(value) => values[idx] = value,
                Err(err) => {
                    errors.push(format!("{line} 行目: {}: {err}", ds.columns[idx].name));
                    bad = true;
                }
            }
        }
        // 省略した列も、必須ならここで引っかかる
        if !bad {
            for (idx, col) in ds.columns.iter().enumerate() {
                if col.required && values[idx] == Value::Null {
                    errors.push(format!("{line} 行目: {}: 必須の列が空です", col.name));
                    bad = true;
                }
            }
        }
        if !bad {
            rows.push(values);
        }
    }
    if !errors.is_empty() {
        let total = errors.len();
        errors.truncate(MAX_REPORTED);
        let more = total - errors.len();
        let mut message = format!(
            "不正な行があるため、何も追記していません:\n  {}",
            errors.join("\n  ")
        );
        if more > 0 {
            message.push_str(&format!("\n  …ほか {more} 件"));
        }
        bail!(message);
    }
    if rows.is_empty() {
        bail!("データ行がありません");
    }

    let verb = match on_conflict {
        OnConflict::Error => "INSERT",
        OnConflict::Skip => "INSERT OR IGNORE",
        OnConflict::Replace => "INSERT OR REPLACE",
    };
    let columns: Vec<String> = ds.columns.iter().map(|c| quote(&c.name)).collect();
    let marks: Vec<String> = (1..=columns.len()).map(|i| format!("?{i}")).collect();
    let insert = format!(
        "{verb} INTO {} ({}) VALUES ({})",
        quote(&ds.name),
        columns.join(", "),
        marks.join(", ")
    );
    let before = db::row_count(&conn, &ds.name)?;
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare(&insert)?;
        for (n, row) in rows.iter().enumerate() {
            stmt.execute(rusqlite::params_from_iter(row)).map_err(|err| {
                if is_unique_violation(&err) {
                    anyhow::anyhow!(
                        "{} 件目の行がキー（{}）で既存の行と重複しています。何も追記していません（--on-conflict skip|replace で扱いを選べます）",
                        n + 1,
                        ds.key.join(", ")
                    )
                } else {
                    anyhow::Error::new(err)
                }
            })?;
        }
    }
    tx.commit()?;
    let after = db::row_count(&conn, &ds.name)?;
    drop(conn);

    let added = after - before;
    let skipped = rows.len() as i64 - added;
    vcs::commit(root, &format!("data append {name} ({added} rows)"));
    match on_conflict {
        OnConflict::Skip if skipped > 0 => {
            println!(
                "追記しました: {name} に {added} 行（重複 {skipped} 行は入れていません。計 {after} 行）"
            )
        }
        OnConflict::Replace if skipped > 0 => {
            println!("追記しました: {name} に {added} 行、{skipped} 行を置き換え（計 {after} 行）")
        }
        _ => println!("追記しました: {name} に {added} 行（計 {after} 行）"),
    }
    Ok(())
}

fn is_unique_violation(err: &rusqlite::Error) -> bool {
    matches!(
        err,
        rusqlite::Error::SqliteFailure(e, _)
            if e.code == rusqlite::ErrorCode::ConstraintViolation
                && e.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
    )
}

fn names(ds: &Dataset) -> String {
    ds.columns
        .iter()
        .map(|c| c.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn sql(root: &Path, query: &str, as_json: bool) -> Result<()> {
    let Some(conn) = db::open_ro(root)? else {
        bail!("データセットがありません（`ika data new` で作ってください）");
    };
    let mut stmt = conn.prepare(query).context("SQL を解釈できません")?;
    if !stmt.readonly() {
        bail!("読み取り専用です（SELECT だけ実行できます）");
    }
    let header: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let count = header.len();
    let mut rows = stmt.query([])?;
    let mut out = io::stdout().lock();
    if as_json {
        let mut all = Vec::new();
        while let Some(row) = rows.next()? {
            let mut object = serde_json::Map::new();
            for (i, name) in header.iter().enumerate() {
                object.insert(name.clone(), json_value(row.get_ref(i)?));
            }
            all.push(serde_json::Value::Object(object));
        }
        writeln!(out, "{}", serde_json::to_string_pretty(&all)?)?;
    } else {
        writeln!(out, "{}", header.join("\t"))?;
        while let Some(row) = rows.next()? {
            let cells: Vec<String> = (0..count)
                .map(|i| Ok(db::render(row.get_ref(i)?)))
                .collect::<Result<_>>()?;
            writeln!(out, "{}", cells.join("\t"))?;
        }
    }
    Ok(())
}

fn json_value(value: ValueRef) -> serde_json::Value {
    match value {
        ValueRef::Null => serde_json::Value::Null,
        ValueRef::Integer(n) => json!(n),
        ValueRef::Real(x) => json!(x),
        ValueRef::Text(bytes) => json!(String::from_utf8_lossy(bytes)),
        ValueRef::Blob(bytes) => json!(format!("<blob {} bytes>", bytes.len())),
    }
}

fn export(root: &Path, name: &str) -> Result<()> {
    let (conn, ds) = open_dataset(root, name)?;
    let columns: Vec<String> = ds.columns.iter().map(|c| quote(&c.name)).collect();
    let mut stmt = conn.prepare(&format!(
        "SELECT {} FROM {} ORDER BY rowid",
        columns.join(", "),
        quote(&ds.name)
    ))?;
    let mut writer = csv::Writer::from_writer(io::stdout().lock());
    writer.write_record(ds.columns.iter().map(|c| c.name.as_str()))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let cells = (0..ds.columns.len())
            .map(|i| {
                Ok(match row.get_ref(i)? {
                    ValueRef::Null => String::new(),
                    ValueRef::Integer(n) => n.to_string(),
                    ValueRef::Real(x) => x.to_string(),
                    ValueRef::Text(bytes) => String::from_utf8_lossy(bytes).into_owned(),
                    ValueRef::Blob(_) => String::new(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        writer.write_record(&cells)?;
    }
    writer.flush()?;
    Ok(())
}

fn delete(root: &Path, name: &str, condition: &str) -> Result<()> {
    if condition.trim().is_empty() {
        bail!("--where が空です（全部消すなら `ika data rm` でデータセットごと消します）");
    }
    let conn = db::open_rw(root, false)?;
    let ds = db::get(&conn, name)?;
    // 条件式に `;` や副問い合わせで別の処理を紛れ込ませない: 1文だけを実行する
    let removed = conn
        .execute(
            &format!("DELETE FROM {} WHERE {condition}", quote(&ds.name)),
            [],
        )
        .context("条件で消せませんでした")?;
    drop(conn);
    vcs::commit(root, &format!("data delete {name} ({removed} rows)"));
    println!("消しました: {name} から {removed} 行");
    Ok(())
}

fn rm(root: &Path, name: &str) -> Result<()> {
    let mut conn = db::open_rw(root, false)?;
    let ds = db::get(&conn, name)?;
    let rows = db::row_count(&conn, &ds.name)?;
    let tx = conn.transaction()?;
    tx.execute(&format!("DROP TABLE {}", quote(&ds.name)), [])?;
    tx.execute("DELETE FROM _datasets WHERE name = ?1", [&ds.name])?;
    tx.commit()?;
    drop(conn);
    vcs::commit(root, &format!("data rm {name}"));
    println!("消しました: {name}（{rows} 行。履歴は git に残っています）");
    Ok(())
}

/// `ika check` 用。`_datasets` の定義と、実際のテーブル・登録簿との食い違いを返す
pub fn check(root: &Path) -> Result<Vec<String>> {
    let mut problems = Vec::new();
    let Some(conn) = db::open_ro(root)? else {
        return Ok(problems);
    };
    let domains = registry::domains(root)?;
    let projects = registry::projects(root)?;
    for ds in db::list(&conn)? {
        let mut report = |msg: String| problems.push(format!("{}: {}: {msg}", db::FILE, ds.name));
        if !is_ident(&ds.name) {
            report("データセット名が不正です".into());
        }
        if !ds.domain.is_empty() && !domains.iter().any(|d| d.name == ds.domain) {
            report(format!("未登録の領域です: {}", ds.domain));
        }
        for p in &ds.projects {
            if !projects.iter().any(|r| &r.name == p) {
                report(format!("未登録のプロダクトです: {p}"));
            }
        }
        for k in &ds.key {
            if !ds.columns.iter().any(|c| &c.name == k) {
                report(format!("key の列が定義にありません: {k}"));
            }
        }
        let actual = db::table_columns(&conn, &ds.name)?;
        let expected: Vec<String> = ds.columns.iter().map(|c| c.name.clone()).collect();
        if actual.is_empty() {
            report("テーブルがありません".into());
        } else if actual != expected {
            report(format!(
                "定義の列（{}）と実際のテーブルの列（{}）が違います",
                expected.join(", "),
                actual.join(", ")
            ));
        }
    }
    for table in db::unregistered_tables(&conn)? {
        problems.push(format!(
            "{}: {table}: _datasets に定義がないテーブルです（`ika data new` で作ってください）",
            db::FILE
        ));
    }
    Ok(problems)
}
