//! データセットの列定義と、CSV の1セルの検証

use std::cmp::Ordering;

use anyhow::{Result, bail};
use chrono::NaiveDate;

/// データセット名・列名の最大長
const MAX_NAME: usize = 48;

/// 列の型
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    String,
    Int,
    Float,
    /// `YYYY-MM-DD`
    Date,
    /// 入力は true / false / 1 / 0。保存は true / false
    Bool,
    Enum(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    pub name: String,
    pub ty: Type,
    pub required: bool,
}

/// 1セルの値。日付は `YYYY-MM-DD` の文字列で持つ（文字列の順が日付の順になる）
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Int(i64),
    Float(f64),
    Bool(bool),
    Text(String),
}

impl Value {
    /// CSV に書く形。空欄は空文字
    pub fn to_cell(&self) -> String {
        match self {
            Value::Null => String::new(),
            Value::Int(n) => n.to_string(),
            Value::Float(x) => x.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Text(s) => s.clone(),
        }
    }

    /// 並べ替え・比較用。空欄はいちばん小さい。型が違うものは文字列で比べる
    pub fn compare(&self, other: &Value) -> Ordering {
        match (self, other) {
            (Value::Null, Value::Null) => Ordering::Equal,
            (Value::Null, _) => Ordering::Less,
            (_, Value::Null) => Ordering::Greater,
            (Value::Int(a), Value::Int(b)) => a.cmp(b),
            (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
            (Value::Text(a), Value::Text(b)) => a.cmp(b),
            (a, b) => match (a.number(), b.number()) {
                (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
                _ => a.to_cell().cmp(&b.to_cell()),
            },
        }
    }

    pub fn number(&self) -> Option<f64> {
        match self {
            Value::Int(n) => Some(*n as f64),
            Value::Float(x) => Some(*x),
            _ => None,
        }
    }
}

/// データセット名・列名は英小文字で始まり、英小文字・数字・アンダースコアだけ
/// （ファイル名と、`--where`・`--agg` の中でそのまま使うので、記号は使わない）
pub fn is_ident(name: &str) -> bool {
    let mut chars = name.chars();
    name.len() <= MAX_NAME
        && chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
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
                    // 定義は登録簿の1セルに `;` 区切りで並べるので、値に `;` は使えない
                    if values.iter().any(|v| v.contains(';')) {
                        bail!("enum の値に `;` は使えません: {spec}");
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

    /// `name:type` の表記に戻す
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

    pub fn is_numeric(&self) -> bool {
        matches!(self.ty, Type::Int | Type::Float)
    }

    /// CSV の1セルを値にする。空欄は省略可の列なら `Null`、必須の列ならエラー
    pub fn parse_cell(&self, raw: &str) -> Result<Value> {
        let text = raw.trim();
        if text.is_empty() {
            if self.required {
                bail!("必須の列が空です");
            }
            return Ok(Value::Null);
        }
        self.parse_value(raw)
    }

    /// 空でない値を列の型で読む（`--where` の値にも使う）
    pub fn parse_value(&self, raw: &str) -> Result<Value> {
        let text = raw.trim();
        Ok(match &self.ty {
            Type::String => Value::Text(raw.to_string()),
            Type::Int => match text.parse::<i64>() {
                Ok(n) => Value::Int(n),
                Err(_) => bail!("整数ではありません: {text}"),
            },
            Type::Float => match text.parse::<f64>() {
                Ok(x) if x.is_finite() => Value::Float(x),
                _ => bail!("数値ではありません: {text}"),
            },
            Type::Date => match NaiveDate::parse_from_str(text, "%Y-%m-%d") {
                Ok(_) if text.len() == 10 => Value::Text(text.to_string()),
                _ => bail!("日付は YYYY-MM-DD にしてください: {text}"),
            },
            Type::Bool => match text.to_ascii_lowercase().as_str() {
                "true" | "1" => Value::Bool(true),
                "false" | "0" => Value::Bool(false),
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
        assert!(Column::parse("kind:enum(a;b|c)").is_err());
    }

    #[test]
    fn validates_cells() {
        let int = Column::parse("n:int").unwrap();
        assert_eq!(int.parse_cell(" 12 ").unwrap(), Value::Int(12));
        assert!(int.parse_cell("1.5").is_err());
        assert!(int.parse_cell("").is_err());
        let opt = Column::parse("n:int?").unwrap();
        assert_eq!(opt.parse_cell("").unwrap(), Value::Null);
        let date = Column::parse("d:date").unwrap();
        assert!(date.parse_cell("2026-10-18").is_ok());
        assert!(date.parse_cell("2026-1-8").is_err());
        assert!(date.parse_cell("先週").is_err());
        let flag = Column::parse("f:bool").unwrap();
        assert_eq!(flag.parse_cell("TRUE").unwrap(), Value::Bool(true));
        assert_eq!(flag.parse_cell("0").unwrap().to_cell(), "false");
        let kind = Column::parse("k:enum(a|b)").unwrap();
        assert!(kind.parse_cell("c").is_err());
    }

    #[test]
    fn compares_by_type() {
        assert_eq!(Value::Int(9).compare(&Value::Int(10)), Ordering::Less);
        assert_eq!(Value::Float(2.5).compare(&Value::Int(2)), Ordering::Greater);
        assert_eq!(Value::Null.compare(&Value::Int(0)), Ordering::Less);
    }
}
