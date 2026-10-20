//! データセットの列定義と、CSV の1セルの検証

use anyhow::{Result, bail};
use chrono::NaiveDate;
use rusqlite::types::Value;
use serde::{Deserialize, Serialize};

/// データセット名・列名の最大長
const MAX_NAME: usize = 48;

/// 列の型。SQLite の型は `sql_type` で決まり、それ以外（date・bool・enum）は CHECK 制約で守る
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Type {
    String,
    Int,
    Float,
    /// `YYYY-MM-DD`
    Date,
    /// 入力は true / false / 1 / 0。保存は 1 / 0
    Bool,
    Enum(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Column {
    pub name: String,
    pub ty: Type,
    pub required: bool,
}

/// データセット名・列名は英小文字で始まり、英小文字・数字・アンダースコアだけ
/// （SQL の識別子としてそのまま使うので、ハイフンは使わない）
pub fn is_ident(name: &str) -> bool {
    let mut chars = name.chars();
    name.len() <= MAX_NAME
        && chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// SQL の識別子として引用する（`is_ident` を通った名前にだけ使う）
pub fn quote(name: &str) -> String {
    format!("\"{name}\"")
}

fn quote_str(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

impl Column {
    /// `name:type` の形。`?` を型の後ろに付けると省略可（例: `url:string?`、`kind:enum(sold|listed)`）
    pub fn parse(spec: &str) -> Result<Column> {
        let Some((name, ty)) = spec.split_once(':') else {
            bail!("列は `名前:型` で指定してください: {spec}");
        };
        if !is_ident(name) {
            bail!(
                "列名は英小文字で始まり、英小文字・数字・アンダースコアだけ（{MAX_NAME} 文字まで）にしてください: {name}"
            );
        }
        let (ty, required) = match ty.strip_suffix('?') {
            Some(ty) => (ty, false),
            None => (ty, true),
        };
        let ty = match ty {
            "string" => Type::String,
            "int" => Type::Int,
            "float" => Type::Float,
            "date" => Type::Date,
            "bool" => Type::Bool,
            _ => match ty.strip_prefix("enum(").and_then(|t| t.strip_suffix(')')) {
                Some(values) => {
                    let values: Vec<String> = values.split('|').map(str::to_string).collect();
                    if values.iter().any(|v| v.trim().is_empty() || v.trim() != v) {
                        bail!("enum の値は空にせず、前後に空白を付けないでください: {spec}");
                    }
                    Type::Enum(values)
                }
                None => bail!(
                    "型が不正です: {ty}（string / int / float / date / bool / enum(a|b) のどれか）"
                ),
            },
        };
        Ok(Column {
            name: name.to_string(),
            ty,
            required,
        })
    }

    /// `name:type` の表記に戻す（`ika data show` 用）
    pub fn spec(&self) -> String {
        let ty = match &self.ty {
            Type::String => "string".to_string(),
            Type::Int => "int".to_string(),
            Type::Float => "float".to_string(),
            Type::Date => "date".to_string(),
            Type::Bool => "bool".to_string(),
            Type::Enum(values) => format!("enum({})", values.join("|")),
        };
        format!("{}:{ty}{}", self.name, if self.required { "" } else { "?" })
    }

    /// CREATE TABLE の列定義
    pub fn ddl(&self) -> String {
        let name = quote(&self.name);
        let sql_type = match self.ty {
            Type::Int | Type::Bool => "INTEGER",
            Type::Float => "REAL",
            Type::String | Type::Date | Type::Enum(_) => "TEXT",
        };
        let mut ddl = format!("{name} {sql_type}");
        if self.required {
            ddl.push_str(" NOT NULL");
        }
        match &self.ty {
            Type::Bool => ddl.push_str(&format!(" CHECK({name} IN (0, 1))")),
            Type::Date => ddl.push_str(&format!(
                " CHECK({name} GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]')"
            )),
            Type::Enum(values) => {
                let list: Vec<String> = values.iter().map(|v| quote_str(v)).collect();
                ddl.push_str(&format!(" CHECK({name} IN ({}))", list.join(", ")));
            }
            _ => {}
        }
        ddl
    }

    /// CSV の1セルを、保存する値にする。空欄は省略可の列なら NULL、必須の列ならエラー
    pub fn parse_cell(&self, raw: &str) -> Result<Value> {
        let text = raw.trim();
        if text.is_empty() {
            if self.required {
                bail!("必須の列が空です");
            }
            return Ok(Value::Null);
        }
        Ok(match &self.ty {
            Type::String => Value::Text(raw.to_string()),
            Type::Int => match text.parse::<i64>() {
                Ok(n) => Value::Integer(n),
                Err(_) => bail!("整数ではありません: {text}"),
            },
            Type::Float => match text.parse::<f64>() {
                Ok(x) if x.is_finite() => Value::Real(x),
                _ => bail!("数値ではありません: {text}"),
            },
            Type::Date => match NaiveDate::parse_from_str(text, "%Y-%m-%d") {
                Ok(_) if text.len() == 10 => Value::Text(text.to_string()),
                _ => bail!("日付は YYYY-MM-DD にしてください: {text}"),
            },
            Type::Bool => match text.to_ascii_lowercase().as_str() {
                "true" | "1" => Value::Integer(1),
                "false" | "0" => Value::Integer(0),
                _ => bail!("真偽は true / false / 1 / 0 のどれかにしてください: {text}"),
            },
            Type::Enum(values) => {
                if !values.iter().any(|v| v == text) {
                    bail!("値は {} のどれかにしてください: {text}", values.join(" / "));
                }
                Value::Text(text.to_string())
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_column_specs() {
        let col = Column::parse("kind:enum(sold|listed)").unwrap();
        assert_eq!(col.ty, Type::Enum(vec!["sold".into(), "listed".into()]));
        assert!(col.required);
        let col = Column::parse("url:string?").unwrap();
        assert!(!col.required);
        assert_eq!(col.spec(), "url:string?");
        assert!(Column::parse("Price:int").is_err());
        assert!(Column::parse("price").is_err());
        assert!(Column::parse("price:money").is_err());
        assert!(Column::parse("kind:enum()").is_err());
    }

    #[test]
    fn validates_cells() {
        let int = Column::parse("n:int").unwrap();
        assert_eq!(int.parse_cell(" 12 ").unwrap(), Value::Integer(12));
        assert!(int.parse_cell("1.5").is_err());
        assert!(int.parse_cell("").is_err());
        let opt = Column::parse("n:int?").unwrap();
        assert_eq!(opt.parse_cell("").unwrap(), Value::Null);
        let date = Column::parse("d:date").unwrap();
        assert!(date.parse_cell("2026-10-18").is_ok());
        assert!(date.parse_cell("2026-1-8").is_err());
        assert!(date.parse_cell("先週").is_err());
        let flag = Column::parse("f:bool").unwrap();
        assert_eq!(flag.parse_cell("TRUE").unwrap(), Value::Integer(1));
        let kind = Column::parse("k:enum(a|b)").unwrap();
        assert!(kind.parse_cell("c").is_err());
    }

    #[test]
    fn ddl_guards_types() {
        let ddl = Column::parse("k:enum(it's|b)").unwrap().ddl();
        assert!(ddl.contains("'it''s'"));
        assert!(Column::parse("d:date").unwrap().ddl().contains("GLOB"));
    }
}
