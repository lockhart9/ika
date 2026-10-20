//! `ika serve` のデータセットのページ（読み取り専用）。名前・値は必ず `esc` を通して埋め込む。

use std::path::Path;

use anyhow::Result;

use super::page::{esc, layout};
use crate::data::db;
use crate::data::schema::quote;

/// 1ページに出す行数（新しい行から）
const LIMIT: usize = 200;

/// トップに出す一覧。データセットが無ければ空
pub fn section(root: &Path) -> Result<String> {
    let Some(conn) = db::open_ro(root)? else {
        return Ok(String::new());
    };
    let datasets = db::list(&conn)?;
    if datasets.is_empty() {
        return Ok(String::new());
    }
    let mut out = String::from("<h2>データ</h2><ul class=\"list\">");
    for ds in datasets {
        out.push_str(&format!(
            "<li><a href=\"/data/{n}\">{n}</a> <span class=\"badge\">{} 行</span> <span class=\"muted\">{}</span></li>",
            db::row_count(&conn, &ds.name)?,
            esc(&ds.summary),
            n = esc(&ds.name),
        ));
    }
    out.push_str("</ul>");
    Ok(out)
}

pub fn page(root: &Path, name: &str) -> Result<Option<String>> {
    let Some(conn) = db::open_ro(root)? else {
        return Ok(None);
    };
    let Some(ds) = db::find(&conn, name)? else {
        return Ok(None);
    };
    let total = db::row_count(&conn, &ds.name)?;

    let mut body = format!(
        "<div class=\"crumbs\"><a href=\"/\">ika</a> / data</div><h1>{}</h1><p>{}</p>",
        esc(&ds.name),
        esc(&ds.summary)
    );
    let mut meta = vec![format!("作成 {}", esc(&ds.created)), format!("{total} 行")];
    if !ds.domain.is_empty() {
        meta.push(format!("領域 {}", esc(&ds.domain)));
    }
    for p in &ds.projects {
        meta.push(format!("<a href=\"/project/{n}\">{n}</a>", n = esc(p)));
    }
    if !ds.key.is_empty() {
        meta.push(format!("key {}", esc(&ds.key.join(", "))));
    }
    body.push_str(&format!("<p class=\"meta\">{}</p>", meta.join(" · ")));

    body.push_str("<h2>列</h2><ul class=\"list\">");
    for col in &ds.columns {
        body.push_str(&format!("<li><code>{}</code></li>", esc(&col.spec())));
    }
    body.push_str("</ul>");

    let (header, rows) = db::fetch(
        &conn,
        &format!(
            "SELECT * FROM {} ORDER BY rowid DESC LIMIT {LIMIT}",
            quote(&ds.name)
        ),
    )?;
    body.push_str(&format!(
        "<h2>行</h2><p class=\"muted\">新しい順に最大 {LIMIT} 行（全 {total} 行）</p>\
         <div class=\"data\"><table><thead><tr>"
    ));
    for h in &header {
        body.push_str(&format!("<th>{}</th>", esc(h)));
    }
    body.push_str("</tr></thead><tbody>");
    for row in rows {
        body.push_str("<tr>");
        for cell in row {
            body.push_str(&format!("<td>{}</td>", esc(&cell)));
        }
        body.push_str("</tr>");
    }
    body.push_str("</tbody></table></div>");
    Ok(Some(layout(&ds.name, "", &body)))
}
