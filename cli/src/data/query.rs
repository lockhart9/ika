//! `ika data query`・`delete` の絞り込みと集計。CSV を読んで、要る行と数字だけを返す。

use std::cmp::Ordering;
use std::collections::HashMap;

use anyhow::{Result, bail};

use super::schema::{Type, Value, is_ident};
use super::store::Dataset;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Contains,
    NotContains,
}

/// `列 演算子 値` の条件
#[derive(Debug)]
pub struct Condition {
    index: usize,
    op: Op,
    /// `None` は空欄（`列=`・`列!=`）
    value: Option<Value>,
    /// `~`・`!~` の比べる文字列（小文字）
    needle: String,
}

/// 2文字の演算子を先に見る
const OPS: [(&str, Op); 8] = [
    ("!=", Op::Ne),
    ("<=", Op::Le),
    (">=", Op::Ge),
    ("!~", Op::NotContains),
    ("=", Op::Eq),
    ("<", Op::Lt),
    (">", Op::Gt),
    ("~", Op::Contains),
];

impl Condition {
    pub fn parse(ds: &Dataset, text: &str) -> Result<Condition> {
        let text = text.trim();
        let end = text
            .find(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'))
            .unwrap_or(text.len());
        let (name, rest) = text.split_at(end);
        let rest = rest.trim_start();
        let Some((symbol, op)) = OPS.iter().find(|(s, _)| rest.starts_with(s)) else {
            bail!("条件は `列 演算子 値` で書いてください（演算子は = != < <= > >= ~ !~）: {text}");
        };
        if !is_ident(name) {
            bail!("条件の列名が不正です: {text}");
        }
        let (index, col) = ds.column(name)?;
        let raw = rest[symbol.len()..].trim();
        if raw.is_empty() {
            if !matches!(op, Op::Eq | Op::Ne) {
                bail!(
                    "空欄を探すときは `{name}=`（空でない行は `{name}!=`）にしてください: {text}"
                );
            }
            return Ok(Condition {
                index,
                op: *op,
                value: None,
                needle: String::new(),
            });
        }
        let (value, needle) = match op {
            Op::Contains | Op::NotContains => (None, raw.to_lowercase()),
            _ => match col.parse_value(raw) {
                Ok(v) => (Some(v), String::new()),
                Err(err) => bail!("条件の値が列 {name}（{}）に合いません: {err}", col.spec()),
            },
        };
        Ok(Condition {
            index,
            op: *op,
            value,
            needle,
        })
    }

    /// 空欄は値との比較・`~`・`!~` に当たらない（`列=` で探す）
    pub fn matches(&self, row: &[Value]) -> bool {
        let cell = &row[self.index];
        match (self.op, &self.value) {
            (Op::Contains | Op::NotContains, _) => {
                if *cell == Value::Null {
                    return false;
                }
                let found = cell.to_cell().to_lowercase().contains(&self.needle);
                found == (self.op == Op::Contains)
            }
            (Op::Eq, None) => *cell == Value::Null,
            (Op::Ne, None) => *cell != Value::Null,
            (_, None) => false,
            (op, Some(value)) => {
                if *cell == Value::Null {
                    return false;
                }
                let ord = cell.compare(value);
                match op {
                    Op::Eq => ord == Ordering::Equal,
                    Op::Ne => ord != Ordering::Equal,
                    Op::Lt => ord == Ordering::Less,
                    Op::Le => ord != Ordering::Greater,
                    Op::Gt => ord == Ordering::Greater,
                    Op::Ge => ord != Ordering::Less,
                    Op::Contains | Op::NotContains => unreachable!(),
                }
            }
        }
    }
}

pub fn parse_conditions(ds: &Dataset, texts: &[String]) -> Result<Vec<Condition>> {
    texts.iter().map(|t| Condition::parse(ds, t)).collect()
}

pub fn matches_all(conditions: &[Condition], row: &[Value]) -> bool {
    conditions.iter().all(|c| c.matches(row))
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Func {
    Count,
    Sum,
    Avg,
    Min,
    Max,
}

/// `count`・`count:列`・`sum:列`・`avg:列`・`min:列`・`max:列`
#[derive(Debug)]
pub struct Agg {
    func: Func,
    /// `None` は行数（`count`）
    index: Option<usize>,
    pub label: String,
    int: bool,
}

impl Agg {
    pub fn parse(ds: &Dataset, text: &str) -> Result<Agg> {
        let text = text.trim();
        let (name, col) = match text.split_once(':') {
            Some((f, c)) => (f, Some(c.trim())),
            None => (text, None),
        };
        let func = match name {
            "count" => Func::Count,
            "sum" => Func::Sum,
            "avg" => Func::Avg,
            "min" => Func::Min,
            "max" => Func::Max,
            _ => bail!(
                "集計は count / count:列 / sum:列 / avg:列 / min:列 / max:列 のどれかにしてください: {text}"
            ),
        };
        let Some(col) = col else {
            if func != Func::Count {
                bail!("{name} には列を付けてください（例: {name}:price）");
            }
            return Ok(Agg {
                func,
                index: None,
                label: "count".into(),
                int: true,
            });
        };
        let (index, column) = ds.column(col)?;
        if matches!(func, Func::Sum | Func::Avg) && !column.is_numeric() {
            bail!("{name} は int・float の列だけに使えます: {}", column.spec());
        }
        Ok(Agg {
            func,
            index: Some(index),
            label: format!("{name}_{col}"),
            int: column.ty == Type::Int,
        })
    }

    fn compute(&self, rows: &[&Vec<Value>]) -> Value {
        let Some(index) = self.index else {
            return Value::Int(rows.len() as i64);
        };
        let values: Vec<&Value> = rows
            .iter()
            .map(|r| &r[index])
            .filter(|v| **v != Value::Null)
            .collect();
        match self.func {
            Func::Count => Value::Int(values.len() as i64),
            _ if values.is_empty() => Value::Null,
            Func::Sum if self.int => Value::Int(
                values
                    .iter()
                    .filter_map(|v| match v {
                        Value::Int(n) => Some(*n),
                        _ => None,
                    })
                    .sum(),
            ),
            Func::Sum => Value::Float(values.iter().filter_map(|v| v.number()).sum()),
            Func::Avg => {
                let sum: f64 = values.iter().filter_map(|v| v.number()).sum();
                Value::Float(round2(sum / values.len() as f64))
            }
            Func::Min => (*values.iter().min_by(|a, b| a.compare(b)).unwrap()).clone(),
            Func::Max => (*values.iter().max_by(|a, b| a.compare(b)).unwrap()).clone(),
        }
    }
}

pub fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// 結果の表
pub struct Table {
    pub header: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

pub struct Spec<'a> {
    pub conditions: &'a [Condition],
    pub cols: &'a [String],
    pub group_by: &'a [String],
    pub aggs: &'a [String],
    pub sort: &'a [String],
}

pub fn run(ds: &Dataset, rows: &[Vec<Value>], spec: &Spec) -> Result<Table> {
    let matched: Vec<&Vec<Value>> = rows
        .iter()
        .filter(|r| matches_all(spec.conditions, r))
        .collect();

    let mut table = if spec.group_by.is_empty() && spec.aggs.is_empty() {
        let indexes = if spec.cols.is_empty() {
            (0..ds.columns.len()).collect::<Vec<_>>()
        } else {
            spec.cols
                .iter()
                .map(|c| ds.column(c).map(|(i, _)| i))
                .collect::<Result<Vec<_>>>()?
        };
        Table {
            header: indexes
                .iter()
                .map(|&i| ds.columns[i].name.clone())
                .collect(),
            rows: matched
                .iter()
                .map(|r| indexes.iter().map(|&i| r[i].clone()).collect())
                .collect(),
        }
    } else {
        if !spec.cols.is_empty() {
            bail!(
                "--cols は集計しないときだけ使えます（集計では --group-by の列と --agg が出ます）"
            );
        }
        let groups = spec
            .group_by
            .iter()
            .map(|c| ds.column(c).map(|(i, _)| i))
            .collect::<Result<Vec<_>>>()?;
        let mut aggs = spec
            .aggs
            .iter()
            .map(|a| Agg::parse(ds, a))
            .collect::<Result<Vec<_>>>()?;
        if aggs.is_empty() {
            aggs.push(Agg::parse(ds, "count")?);
        }
        let mut order: Vec<Vec<Value>> = Vec::new();
        let mut members: HashMap<Vec<String>, Vec<&Vec<Value>>> = HashMap::new();
        for row in &matched {
            let key: Vec<Value> = groups.iter().map(|&i| row[i].clone()).collect();
            let text: Vec<String> = key.iter().map(Value::to_cell).collect();
            members
                .entry(text)
                .or_insert_with(|| {
                    order.push(key);
                    Vec::new()
                })
                .push(row);
        }
        // グループが無くても、全体の集計は 1 行出す（0 件なら count は 0）
        if groups.is_empty() && order.is_empty() {
            order.push(Vec::new());
            members.insert(Vec::new(), Vec::new());
        }
        order.sort_by(|a, b| compare_rows(a, b));
        let mut header: Vec<String> = groups.iter().map(|&i| ds.columns[i].name.clone()).collect();
        header.extend(aggs.iter().map(|a| a.label.clone()));
        let rows = order
            .into_iter()
            .map(|key| {
                let text: Vec<String> = key.iter().map(Value::to_cell).collect();
                let group = &members[&text];
                let mut row = key;
                row.extend(aggs.iter().map(|a| a.compute(group)));
                row
            })
            .collect();
        Table { header, rows }
    };

    sort(&mut table, spec.sort)?;
    Ok(table)
}

fn compare_rows(a: &[Value], b: &[Value]) -> Ordering {
    a.iter()
        .zip(b)
        .map(|(x, y)| x.compare(y))
        .find(|o| *o != Ordering::Equal)
        .unwrap_or(Ordering::Equal)
}

/// `列`・`-列`（降順）。出力の列名で指定する
fn sort(table: &mut Table, keys: &[String]) -> Result<()> {
    let mut plan = Vec::new();
    for key in keys {
        let key = key.trim();
        let (name, desc) = match key.strip_prefix('-') {
            Some(name) => (name, true),
            None => (key, false),
        };
        let Some(index) = table.header.iter().position(|h| h == name) else {
            bail!(
                "--sort の列が出力にありません: {name}（出力の列: {}）",
                table.header.join(", ")
            );
        };
        plan.push((index, desc));
    }
    if plan.is_empty() {
        return Ok(());
    }
    table.rows.sort_by(|a, b| {
        plan.iter()
            .map(|&(i, desc)| {
                let ord = a[i].compare(&b[i]);
                if desc { ord.reverse() } else { ord }
            })
            .find(|o| *o != Ordering::Equal)
            .unwrap_or(Ordering::Equal)
    });
    Ok(())
}
