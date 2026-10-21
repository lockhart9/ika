//! タグ一覧（`/tags`）とタグ別のページ（`/tags/<タグ>`）

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;

use super::http::{Request, percent_encode};
use super::page::{Doc, doc_item, docs, esc, layout, search_url};

/// タグ別のページの並び（種類 → 見出し）。ここにない種類は「その他」
const GROUPS: [(&str, &str); 6] = [
    ("decision", "決定"),
    ("insight", "知見"),
    ("project", "プロダクト概要"),
    ("spec", "要件"),
    ("note", "ノート"),
    ("source", "取り込んだ資料"),
];

/// トップに出すタグの数
const TOP: usize = 24;

pub fn tag_url(tag: &str) -> String {
    format!("/tags/{}", percent_encode(tag))
}

/// タグごとの件数。件数の多い順、同数なら名前順
fn counts<'a>(tags: impl Iterator<Item = &'a String>) -> Vec<(&'a str, usize)> {
    let mut map: BTreeMap<&str, usize> = BTreeMap::new();
    for tag in tags {
        *map.entry(tag.as_str()).or_default() += 1;
    }
    let mut list: Vec<_> = map.into_iter().collect();
    list.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    list
}

fn chips(tags: &[(&str, usize)]) -> String {
    let mut out = String::from("<ul class=\"tags\">");
    for (tag, n) in tags {
        out.push_str(&format!(
            "<li><a href=\"{}\">#{}<span class=\"n\">{n}</span></a></li>",
            tag_url(tag),
            esc(tag)
        ));
    }
    out.push_str("</ul>");
    out
}

/// トップの「タグ」の節（件数の多いものだけ）
pub fn section(docs: &[Doc]) -> String {
    let all = counts(docs.iter().flat_map(|d| &d.tags));
    if all.is_empty() {
        return String::new();
    }
    let more = if all.len() > TOP {
        format!(
            "<p><a href=\"/tags\">すべてのタグ（{} 個）</a></p>",
            all.len()
        )
    } else {
        String::new()
    };
    format!("<h2>タグ</h2>{}{more}", chips(&all[..all.len().min(TOP)]))
}

pub fn index(root: &Path, req: &Request) -> Result<String> {
    let docs = docs(root)?;
    let mut all = counts(docs.iter().flat_map(|d| &d.tags));
    let by_name = req.param("sort") == Some("name");
    if by_name {
        all.sort_by(|a, b| a.0.cmp(b.0));
    }
    let sort = if by_name {
        "<a href=\"/tags\">件数順</a> · 名前順"
    } else {
        "件数順 · <a href=\"/tags?sort=name\">名前順</a>"
    };
    let body = format!(
        "<div class=\"crumbs\"><a href=\"/\">ika</a> / タグ</div><h1>タグ</h1>\
         <p class=\"muted\">{} 個 · {sort}</p>{}",
        all.len(),
        chips(&all)
    );
    Ok(layout("タグ", "", &body))
}

/// そのタグを持つページがなければ `None`
pub fn page(root: &Path, tag: &str) -> Result<Option<String>> {
    let docs = docs(root)?;
    let mut tagged: Vec<&Doc> = docs
        .iter()
        .filter(|d| d.tags.iter().any(|t| t == tag))
        .collect();
    if tagged.is_empty() {
        return Ok(None);
    }
    tagged.sort_by(|a, b| {
        (std::cmp::Reverse(&a.date), &a.rel).cmp(&(std::cmp::Reverse(&b.date), &b.rel))
    });

    let mut body = format!(
        "<div class=\"crumbs\"><a href=\"/\">ika</a> / <a href=\"/tags\">タグ</a> / #{0}</div>\
         <h1>#{0}</h1><p class=\"muted\">{1} 件 · <a href=\"{2}\">このタグで検索</a></p>",
        esc(tag),
        tagged.len(),
        esc(&search_url("tag", tag))
    );

    // 一緒に付いているタグ（このタグを持つページ内での件数順）
    let related = counts(
        tagged
            .iter()
            .flat_map(|d| &d.tags)
            .filter(|t| t.as_str() != tag),
    );
    if !related.is_empty() {
        body.push_str("<h2>一緒に付いているタグ</h2>");
        body.push_str(&chips(&related));
    }

    let known = |kind: &str| GROUPS.iter().any(|(k, _)| *k == kind);
    let groups = GROUPS
        .iter()
        .map(|(kind, label)| {
            (
                *label,
                tagged
                    .iter()
                    .filter(|d| d.kind == *kind)
                    .collect::<Vec<_>>(),
            )
        })
        .chain([(
            "その他",
            tagged.iter().filter(|d| !known(&d.kind)).collect(),
        )]);
    for (label, items) in groups {
        if items.is_empty() {
            continue;
        }
        body.push_str(&format!(
            "<h2>{label}（{} 件）</h2><ul class=\"list\">",
            items.len()
        ));
        for doc in items {
            body.push_str(&doc_item(doc));
        }
        body.push_str("</ul>");
    }
    Ok(Some(layout(&format!("#{tag}"), "", &body)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_sorts_by_count_then_name() {
        let tags: Vec<String> = ["b", "a", "b", "c", "a"].map(String::from).into();
        assert_eq!(counts(tags.iter()), vec![("a", 2), ("b", 2), ("c", 1)]);
    }

    #[test]
    fn tag_url_encodes_the_tag() {
        assert_eq!(tag_url("労務 a&b"), "/tags/%E5%8A%B4%E5%8B%99%20a%26b");
    }
}
