//! `ika data`: 表形式データ（価格の履歴・計測値・ログなど）を CSV に溜める。
//! 定義は登録簿 `data/datasets.csv`、行は `data/datasets/<name>.csv`。
//! 書き込みは検証を通すため `ika data` だけで行い、読むときは `show`・`query` で要る分だけ出す。

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::{Subcommand, ValueEnum};
use serde_json::json;

use crate::kb::atomic;
use crate::registry;
use crate::vcs;

pub mod query;
pub mod schema;
pub mod store;

use schema::{Column, Type, Value, is_ident};
use store::Dataset;

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
    /// 定義・行数・列ごとの要約・末尾の数行を見る
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
    /// 条件で絞り込み、列を選び、グループごとに集計する（全行を読まずに要る分だけ出す）
    Query {
        name: String,
        /// 条件 `列 演算子 値`（= != < <= > >= ~ !~。~ は含む）。繰り返すと AND。`列=` は空欄の行
        #[arg(short = 'w', long = "where")]
        conditions: Vec<String>,
        /// 出す列（カンマ区切り。集計しないとき）
        #[arg(long, value_delimiter = ',')]
        cols: Vec<String>,
        /// グループにする列（カンマ区切り）
        #[arg(long, value_delimiter = ',')]
        group_by: Vec<String>,
        /// 集計 count / count:列 / sum:列 / avg:列 / min:列 / max:列（カンマ区切り）
        #[arg(long, value_delimiter = ',')]
        agg: Vec<String>,
        /// 並べ替える出力の列（カンマ区切り。`-列` で降順）
        #[arg(long, value_delimiter = ',', allow_hyphen_values = true)]
        sort: Vec<String>,
        /// 出す行の上限（0 で全部）
        #[arg(long, default_value_t = 50)]
        limit: usize,
        /// JSON の配列で出力する（既定はタブ区切り）
        #[arg(long)]
        json: bool,
    },
    /// CSV として標準出力へ書き出す（移行・共有用）
    Export { name: String },
    /// 条件に合う行を消す（条件は query と同じ。繰り返すと AND）。間違った行の訂正は、消して追記し直す
    Delete {
        name: String,
        #[arg(short = 'w', long = "where", required = true)]
        conditions: Vec<String>,
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
        Command::Query {
            name,
            conditions,
            cols,
            group_by,
            agg,
            sort,
            limit,
            json,
        } => {
            let ds = store::get(root, &name)?;
            let conditions = query::parse_conditions(&ds, &conditions)?;
            let rows = store::rows(root, &ds)?;
            let table = query::run(
                &ds,
                &rows,
                &query::Spec {
                    conditions: &conditions,
                    cols: &cols,
                    group_by: &group_by,
                    aggs: &agg,
                    sort: &sort,
                },
            )?;
            print_table(table, limit, json)
        }
        Command::Export { name } => export(root, &name),
        Command::Delete { name, conditions } => delete(root, &name, &conditions),
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
    let mut entries = store::entries(root)?;
    if entries.iter().any(|e| e.name == dataset.name) {
        bail!("すでにあります: {}", dataset.name);
    }
    let rows = store::rows_path(root, &dataset.name);
    fs::create_dir_all(root.join(store::DIR))?;
    atomic::create_new(&rows, &store::rows_text(&dataset, &[])?)?;
    entries.push(dataset.entry());
    store::save_entries(root, &entries)?;

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
    for ds in store::list(root)? {
        println!(
            "{:<24} {:>7} 行  {}",
            ds.name,
            store::row_count(root, &ds)?,
            ds.summary
        );
    }
    Ok(())
}

/// 表示用。タブと改行は空白にして、1行1レコードを保つ
fn render(value: &Value) -> String {
    value.to_cell().replace(['\t', '\n', '\r'], " ")
}

fn number(x: f64) -> String {
    query::round2(x).to_string()
}

/// 列ごとの要約（`show` 用）。全行を出さずに、値の形と範囲が分かるようにする
fn summarize(col: &Column, index: usize, rows: &[Vec<Value>]) -> String {
    let values: Vec<&Value> = rows
        .iter()
        .map(|r| &r[index])
        .filter(|v| **v != Value::Null)
        .collect();
    let mut parts = Vec::new();
    if !col.required {
        parts.push(format!("空欄 {}", rows.len() - values.len()));
    }
    if !values.is_empty() {
        let min = values.iter().min_by(|a, b| a.compare(b)).unwrap();
        let max = values.iter().max_by(|a, b| a.compare(b)).unwrap();
        match &col.ty {
            Type::Int | Type::Float => {
                let sum: f64 = values.iter().filter_map(|v| v.number()).sum();
                parts.push(format!(
                    "最小 {} / 最大 {} / 平均 {}",
                    render(min),
                    render(max),
                    number(sum / values.len() as f64)
                ));
            }
            Type::Date => parts.push(format!("最小 {} / 最大 {}", render(min), render(max))),
            Type::Enum(options) => {
                let counts: Vec<String> = options
                    .iter()
                    .map(|o| {
                        let n = values.iter().filter(|v| v.to_cell() == *o).count();
                        format!("{o} {n}")
                    })
                    .collect();
                parts.push(counts.join(" / "));
            }
            Type::Bool => {
                let yes = values.iter().filter(|v| **v == &Value::Bool(true)).count();
                parts.push(format!("true {yes} / false {}", values.len() - yes));
            }
            Type::String => {
                let kinds: HashSet<String> = values.iter().map(|v| v.to_cell()).collect();
                parts.push(format!("{} 種類", kinds.len()));
            }
        }
    }
    if parts.is_empty() {
        col.spec()
    } else {
        format!("{}  {}", col.spec(), parts.join(" · "))
    }
}

fn show(root: &Path, name: &str, tail: usize) -> Result<()> {
    let ds = store::get(root, name)?;
    let rows = store::rows(root, &ds)?;
    println!("{}  {}", ds.name, ds.summary);
    println!("作成: {}", ds.created);
    if !ds.domain.is_empty() {
        println!("領域: {}", ds.domain);
    }
    if !ds.projects.is_empty() {
        println!("プロダクト: {}", ds.projects.join(", "));
    }
    println!("行数: {}", rows.len());
    if !ds.key.is_empty() {
        println!("key: {}", ds.key.join(", "));
    }
    println!("列:");
    for (i, col) in ds.columns.iter().enumerate() {
        println!("  {}", summarize(col, i, &rows));
    }
    if tail > 0 && !rows.is_empty() {
        let start = rows.len().saturating_sub(tail);
        println!("末尾 {} 行:", rows.len() - start);
        println!("  {}", ds.names().join("\t"));
        for row in &rows[start..] {
            let cells: Vec<String> = row.iter().map(render).collect();
            println!("  {}", cells.join("\t"));
        }
    }
    println!("絞り込み・集計は `ika data query {name} -w … --group-by … --agg …`");
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

fn report(errors: Vec<String>, lead: &str) -> anyhow::Error {
    let total = errors.len();
    let shown: Vec<String> = errors.into_iter().take(MAX_REPORTED).collect();
    let more = total - shown.len();
    let mut message = format!("{lead}:\n  {}", shown.join("\n  "));
    if more > 0 {
        message.push_str(&format!("\n  …ほか {more} 件"));
    }
    anyhow::anyhow!(message)
}

/// 標準入力の CSV を、定義の列順・CSV に書く形の行にする。1行でも不正なら失敗する
fn parse_input(ds: &Dataset, input: &str) -> Result<Vec<Vec<String>>> {
    let mut reader = csv::Reader::from_reader(input.as_bytes());
    let headers = reader.headers().context("ヘッダ行を読めません")?.clone();
    // CSV の列 -> 定義の列の位置
    let mut position = Vec::new();
    for header in &headers {
        let header = header.trim();
        let Some(idx) = ds.columns.iter().position(|c| c.name == header) else {
            bail!(
                "定義にない列です: {header}（列: {}）",
                ds.names().join(", ")
            );
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

    let mut rows = Vec::new();
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
            rows.push(values.iter().map(Value::to_cell).collect());
        }
    }
    if !errors.is_empty() {
        return Err(report(errors, "不正な行があるため、何も追記していません"));
    }
    if rows.is_empty() {
        bail!("データ行がありません");
    }
    Ok(rows)
}

fn append(root: &Path, name: &str, on_conflict: OnConflict) -> Result<()> {
    let input = read_stdin()?;
    let ds = store::get(root, name)?;
    if on_conflict != OnConflict::Error && ds.key.is_empty() {
        bail!("--on-conflict は key があるデータセットだけで使えます（重複の判定がありません）");
    }
    let incoming = parse_input(&ds, &input)?;
    let positions = ds.key_positions();

    let (mut added, mut skipped, mut replaced, mut total) = (0, 0, 0, 0);
    atomic::modify(&store::rows_path(root, &ds.name), |text| {
        let mut rows = store::parse_rows(&ds, text)?;
        // key -> 行の位置（既存の行と、この入力で足した行）
        let mut index: HashMap<Vec<String>, usize> = HashMap::new();
        if !positions.is_empty() {
            for (i, row) in rows.iter().enumerate() {
                index.insert(store::key_of(&positions, row), i);
            }
        }
        for (n, row) in incoming.into_iter().enumerate() {
            if positions.is_empty() {
                rows.push(row);
                added += 1;
                continue;
            }
            let key = store::key_of(&positions, &row);
            match (index.get(&key).copied(), on_conflict) {
                (None, _) => {
                    index.insert(key, rows.len());
                    rows.push(row);
                    added += 1;
                }
                (Some(_), OnConflict::Error) => bail!(
                    "{} 件目の行がキー（{}）で既存の行と重複しています。何も追記していません（--on-conflict skip|replace で扱いを選べます）",
                    n + 1,
                    ds.key.join(", ")
                ),
                (Some(_), OnConflict::Skip) => skipped += 1,
                (Some(i), OnConflict::Replace) => {
                    rows[i] = row;
                    replaced += 1;
                }
            }
        }
        total = rows.len();
        store::rows_text(&ds, &rows)
    })?;

    vcs::commit(root, &format!("data append {name} ({added} rows)"));
    if skipped > 0 {
        println!(
            "追記しました: {name} に {added} 行（重複 {skipped} 行は入れていません。計 {total} 行）"
        );
    } else if replaced > 0 {
        println!("追記しました: {name} に {added} 行、{replaced} 行を置き換え（計 {total} 行）");
    } else {
        println!("追記しました: {name} に {added} 行（計 {total} 行）");
    }
    Ok(())
}

fn json_value(value: &Value) -> serde_json::Value {
    match value {
        Value::Null => serde_json::Value::Null,
        Value::Int(n) => json!(n),
        Value::Float(x) => json!(x),
        Value::Bool(b) => json!(b),
        Value::Text(s) => json!(s),
    }
}

fn print_table(mut table: query::Table, limit: usize, as_json: bool) -> Result<()> {
    let total = table.rows.len();
    if limit > 0 && total > limit {
        table.rows.truncate(limit);
    }
    let mut out = io::stdout().lock();
    if as_json {
        let all: Vec<serde_json::Value> = table
            .rows
            .iter()
            .map(|row| {
                let object: serde_json::Map<String, serde_json::Value> = table
                    .header
                    .iter()
                    .zip(row)
                    .map(|(h, v)| (h.clone(), json_value(v)))
                    .collect();
                serde_json::Value::Object(object)
            })
            .collect();
        writeln!(out, "{}", serde_json::to_string_pretty(&all)?)?;
    } else {
        writeln!(out, "{}", table.header.join("\t"))?;
        for row in &table.rows {
            let cells: Vec<String> = row.iter().map(render).collect();
            writeln!(out, "{}", cells.join("\t"))?;
        }
    }
    out.flush()?;
    if total > table.rows.len() {
        eprintln!(
            "…ほか {} 行（全 {total} 行。--limit で増やせる。--limit 0 で全部）",
            total - table.rows.len()
        );
    }
    Ok(())
}

fn export(root: &Path, name: &str) -> Result<()> {
    let ds = store::get(root, name)?;
    // 書き込みは常に `rows_text` を通るので、ファイルの本文がそのまま追記に渡し直せる形
    let raw = store::raw_rows(root, &ds)?;
    let mut out = io::stdout().lock();
    out.write_all(store::rows_text(&ds, &raw)?.as_bytes())?;
    out.flush()?;
    Ok(())
}

fn delete(root: &Path, name: &str, conditions: &[String]) -> Result<()> {
    let ds = store::get(root, name)?;
    let conditions = query::parse_conditions(&ds, conditions)?;
    if conditions.is_empty() {
        bail!("条件（-w）が要ります（全部消すなら `ika data rm` でデータセットごと消します）");
    }
    let mut removed = 0;
    atomic::modify(&store::rows_path(root, &ds.name), |text| {
        let raw = store::parse_rows(&ds, text)?;
        let typed = store::typed(&ds, &raw)?;
        let kept: Vec<Vec<String>> = raw
            .into_iter()
            .zip(&typed)
            .filter(|(_, row)| !query::matches_all(&conditions, row))
            .map(|(raw, _)| raw)
            .collect();
        removed = typed.len() - kept.len();
        store::rows_text(&ds, &kept)
    })?;
    if removed > 0 {
        vcs::commit(root, &format!("data delete {name} ({removed} rows)"));
    }
    println!("消しました: {name} から {removed} 行");
    Ok(())
}

fn rm(root: &Path, name: &str) -> Result<()> {
    let ds = store::get(root, name)?;
    let rows = store::row_count(root, &ds).unwrap_or(0);
    let entries: Vec<store::Entry> = store::entries(root)?
        .into_iter()
        .filter(|e| e.name != ds.name)
        .collect();
    store::save_entries(root, &entries)?;
    let file = store::rows_path(root, &ds.name);
    if file.exists() {
        fs::remove_file(&file)?;
    }
    vcs::commit(root, &format!("data rm {name}"));
    println!("消しました: {name}（{rows} 行。履歴は git に残っています）");
    Ok(())
}

/// `ika check` 用。登録簿の定義と行ファイルの食い違い・型に合わないセル・key の重複を返す
pub fn check(root: &Path) -> Result<Vec<String>> {
    let mut problems = Vec::new();
    if root.join(store::LEGACY).exists() {
        problems.push(format!(
            "{}: 以前の形式（SQLite）のデータセットです。CSV に移してください（手順は ika のリポジトリの docs/data-format.md「以前の形式から移す」）",
            store::LEGACY
        ));
    }
    let entries = match store::entries(root) {
        Ok(entries) => entries,
        Err(err) => {
            problems.push(format!("{}: {err:#}", store::REGISTRY));
            return Ok(problems);
        }
    };
    let domains = registry::domains(root)?;
    let projects = registry::projects(root)?;
    let files = store::row_files(root)?;
    let mut seen = HashSet::new();
    for entry in &entries {
        let mut report = |msg: String| {
            problems.push(format!("{}: {}: {msg}", store::REGISTRY, entry.name));
        };
        if !seen.insert(entry.name.clone()) {
            report("同じ名前の定義が 2 つあります".into());
            continue;
        }
        if !is_ident(&entry.name) {
            report("データセット名が不正です".into());
            continue;
        }
        let ds = match entry.dataset() {
            Ok(ds) => ds,
            Err(err) => {
                report(format!("列の定義が不正です: {err}"));
                continue;
            }
        };
        if !ds.domain.is_empty() && !domains.iter().any(|d| d.name == ds.domain) {
            report(format!("未登録の領域です: {}", ds.domain));
        }
        for p in &ds.projects {
            if !projects.iter().any(|r| &r.name == p) {
                report(format!("未登録のプロダクトです: {p}"));
            }
        }
        for k in &ds.key {
            match ds.columns.iter().find(|c| &c.name == k) {
                None => report(format!("key の列が定義にありません: {k}")),
                Some(c) if !c.required => report(format!("key の列が省略可です: {k}")),
                Some(_) => {}
            }
        }
        if !files.contains(&ds.name) {
            report(format!(
                "行のファイルがありません: {}",
                store::rows_rel(&ds.name)
            ));
            continue;
        }
        let rel = store::rows_rel(&ds.name);
        let raw = match store::read_text(root, &ds).and_then(|t| store::parse_rows(&ds, &t)) {
            Ok(raw) => raw,
            Err(err) => {
                problems.push(format!("{rel}: {err:#}"));
                continue;
            }
        };
        let positions = ds.key_positions();
        let mut keys: HashMap<Vec<String>, usize> = HashMap::new();
        let mut errors = Vec::new();
        for (i, row) in raw.iter().enumerate() {
            let line = i + 2;
            for (cell, col) in row.iter().zip(&ds.columns) {
                if let Err(err) = col.parse_cell(cell) {
                    errors.push(format!("{rel}: {line} 行目: {}: {err}", col.name));
                }
            }
            if !positions.is_empty() {
                let key = store::key_of(&positions, row);
                if let Some(first) = keys.insert(key.clone(), line) {
                    errors.push(format!(
                        "{rel}: {line} 行目: key（{}）が {first} 行目と重複しています: {}",
                        ds.key.join(", "),
                        key.join(", ")
                    ));
                }
            }
        }
        let total = errors.len();
        problems.extend(errors.into_iter().take(MAX_REPORTED));
        if total > MAX_REPORTED {
            problems.push(format!("{rel}: …ほか {} 件", total - MAX_REPORTED));
        }
    }
    let mut stray: Vec<&String> = files.iter().filter(|f| !seen.contains(*f)).collect();
    stray.sort();
    for name in stray {
        problems.push(format!(
            "{}: {} に定義がない行のファイルです（`ika data new` で作ってください）",
            store::rows_rel(name),
            store::REGISTRY
        ));
    }
    Ok(problems)
}
