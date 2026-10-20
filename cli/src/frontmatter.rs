use std::collections::BTreeMap;

use anyhow::{Result, bail};

/// `---` で囲まれた先頭ブロックを `key: value` の組として読む。
/// 値は1行・リストは `[a, b]` のインライン形式だけを受け付ける（docs/kb-format.md の規約）。
pub fn parse(text: &str) -> Result<Option<BTreeMap<String, String>>> {
    let mut lines = text.lines();
    if lines.next().map(str::trim_end) != Some("---") {
        return Ok(None);
    }
    let mut fields = BTreeMap::new();
    for line in lines {
        if line.trim_end() == "---" {
            return Ok(Some(fields));
        }
        if line.trim().is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            bail!("フロントマターの行が `key: value` ではありません: {line}");
        };
        let key = key.trim();
        if key.is_empty() || line.starts_with([' ', '\t']) {
            bail!("フロントマターはネストできません: {line}");
        }
        if fields
            .insert(key.to_string(), value.trim().to_string())
            .is_some()
        {
            bail!("フロントマターのキーが重複しています: {key}");
        }
    }
    bail!("フロントマターが `---` で閉じられていません")
}

/// `[a, b]` を要素のリストにする
pub fn list(value: &str) -> Result<Vec<String>> {
    let Some(inner) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) else {
        bail!("リストは `[a, b]` の形式で書いてください: {value}");
    };
    Ok(inner
        .split(',')
        .map(|item| item.trim().trim_matches(['"', '\'']).to_string())
        .filter(|item| !item.is_empty())
        .collect())
}

/// 本文（フロントマターを除いた部分）を返す
pub fn body(text: &str) -> &str {
    let Some(rest) = text.strip_prefix("---\n") else {
        return text;
    };
    match rest.find("\n---\n") {
        Some(end) => &rest[end + 5..],
        None => text,
    }
}

/// フロントマター（閉じの `---` の行まで）を返す。無ければ空文字列
pub fn head(text: &str) -> &str {
    &text[..text.len() - body(text).len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fields_and_lists() {
        let text = "---\ntitle: A: B\nprojects: [foo, bar]\ntags: []\nsource: https://x.test/#a\n---\nbody\n";
        let fields = parse(text).unwrap().unwrap();
        assert_eq!(fields["title"], "A: B");
        assert_eq!(fields["source"], "https://x.test/#a");
        assert_eq!(list(&fields["projects"]).unwrap(), ["foo", "bar"]);
        assert!(list(&fields["tags"]).unwrap().is_empty());
        assert_eq!(body(text), "body\n");
        assert_eq!(head(text), &text[..text.len() - "body\n".len()]);
        assert_eq!(head("# no frontmatter"), "");
    }

    #[test]
    fn rejects_missing_or_unclosed_block() {
        assert!(parse("# no frontmatter").unwrap().is_none());
        assert!(parse("---\ntitle: x\n").is_err());
        assert!(parse("---\ntags:\n  - a\n---\n").is_err());
    }
}
