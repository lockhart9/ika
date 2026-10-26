//! 書いた直後の助言（`ika new`・`ika kb write`）: 似た既存のナレッジと、リンクを足す README の候補。
//! 標準エラーに出すだけの助言で、失敗しても書き込みには影響しない（何も出さずに終わる）。
//!
//! 似ているかは、タイトルとタグの内容語（key）が既存のファイルにいくつ含まれるかで決める。
//! 本文の冒頭の語は、候補を関連度順に並べるときの手がかりにだけ使う。

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use crate::kb::fts::{self, Args};
use crate::{check, frontmatter, kb};

/// 候補にする件数
const TOP: usize = 3;
/// 本文から語を取る長さ（文字数）
const BODY_CHARS: usize = 300;
/// 共通語の下限（key の語数に対する割合）と、割合に関わらない最小値
const MIN_RATIO: f64 = 0.4;
const MIN_SHARED: usize = 2;
/// 1 候補に添える共通の語の数
const SHOWN_WORDS: usize = 4;
/// KB がこの件数以上あるとき、全体のこの割合より多くのファイルに出る語は「一般的な語」として key から外す
/// （「調査」「まとめ」のような語だけで似ているとは言えない）。小さな KB では割合が当てにならないので外さない
const GENERIC_MIN_DOCS: usize = 30;
const GENERIC_RATIO: f64 = 0.35;

/// `rel` を新しく作ったあとに呼ぶ。`text` はその全文
pub fn after_write(root: &Path, rel: &Path, text: &str) {
    let _ = suggest(root, rel, text);
}

struct Meta {
    kind: String,
    title: String,
    tags: Vec<String>,
    projects: Vec<String>,
}

fn meta(text: &str) -> Option<Meta> {
    let fields = frontmatter::parse(text).ok().flatten()?;
    let field = |key: &str| fields.get(key).cloned().unwrap_or_default();
    let list = |key: &str| {
        fields
            .get(key)
            .and_then(|v| frontmatter::list(v).ok())
            .unwrap_or_default()
    };
    Some(Meta {
        kind: field("type"),
        title: field("title"),
        tags: list("tags"),
        projects: list("projects"),
    })
}

fn suggest(root: &Path, rel: &Path, text: &str) -> anyhow::Result<()> {
    let Some(me) = meta(text).filter(|m| {
        matches!(m.kind.as_str(), "note" | "insight" | "decision" | "spec") && !m.title.is_empty()
    }) else {
        return Ok(());
    };

    let mut out = String::new();
    let similar = similar(root, rel, text, &me)?;
    if !similar.is_empty() {
        out.push_str("似た既存のナレッジ:\n");
        for s in &similar {
            out.push_str(&format!(
                "  {}  [{}] {}  {}\n      共通の語: {}\n",
                s.rel, s.label, s.date, s.title, s.words
            ));
        }
        out.push_str("同じ話題なら新規より追記を（ika kb append / edit）。関連するなら本文からリンクを張る\n");
    }

    // README の「関連」に足す候補（spec は README の関連に並べない）
    if me.kind != "spec" {
        for project in &me.projects {
            let readme = crate::registry::readme_path(root, project);
            let Ok(readme_text) = fs::read_to_string(&readme) else {
                continue;
            };
            let readme_rel = readme.strip_prefix(root).unwrap_or(&readme);
            let dir = readme_rel.parent().unwrap_or(Path::new(""));
            let linked = check::link_targets(&readme_text)
                .iter()
                .any(|t| kb::normalize(&dir.join(t)) == rel);
            if !linked {
                out.push_str(&format!(
                    "README の「関連」に足す候補: {}\n",
                    readme_rel.display()
                ));
            }
        }
    }

    if !out.is_empty() {
        eprint!("\n{out}");
    }
    Ok(())
}

struct Similar {
    rel: String,
    label: String,
    date: String,
    title: String,
    words: String,
}

fn similar(root: &Path, rel: &Path, text: &str, me: &Meta) -> anyhow::Result<Vec<Similar>> {
    // key: タイトルとタグの内容語。候補はこの語を一定数含むものだけ
    let key_source = format!("{} {}", me.title, me.tags.join(" "));
    let mut key = fts::content_words(&key_source);
    let docs = fts::find(root, &Args::default())?.total;
    if docs >= GENERIC_MIN_DOCS {
        let mut specific = Vec::new();
        for word in key {
            let hits = fts::find(
                root,
                &Args {
                    terms: vec![word.0.clone()],
                    limit: Some(1),
                    ..Args::default()
                },
            )?
            .total;
            if (hits as f64) <= docs as f64 * GENERIC_RATIO {
                specific.push(word);
            }
        }
        key = specific;
    }
    if key.len() < MIN_SHARED {
        return Ok(Vec::new());
    }
    let need = MIN_SHARED.max((key.len() as f64 * MIN_RATIO).ceil() as usize);

    // 検索に使う語: key に本文の冒頭の語を足す（並べ替えの手がかり）
    let body: String = frontmatter::body(text).chars().take(BODY_CHARS).collect();
    let mut terms: Vec<String> = Vec::new();
    let mut seen = BTreeSet::new();
    for (surface, base) in key.iter().chain(fts::content_words(&body).iter()) {
        if seen.insert(base.clone()) {
            terms.push(surface.clone());
        }
    }
    let found = fts::find(
        root,
        &Args {
            terms,
            any: true,
            limit: Some(40),
            ..Args::default()
        },
    )?;

    let mut out = Vec::new();
    for hit in found.hits {
        if Path::new(&hit.rel) == rel {
            continue;
        }
        let Ok(other) = fs::read_to_string(root.join(&hit.rel)) else {
            continue;
        };
        let Some(m) = meta(&other).filter(|m| m.kind != "source") else {
            continue;
        };
        let lower = other.to_lowercase();
        let shared: Vec<&str> = key
            .iter()
            .filter(|(surface, base)| {
                lower.contains(surface.as_str()) || lower.contains(base.as_str())
            })
            .map(|(surface, _)| surface.as_str())
            .collect();
        if shared.len() < need {
            continue;
        }
        out.push(Similar {
            rel: hit.rel,
            label: hit.label,
            date: hit.date,
            title: m.title,
            words: shared
                .iter()
                .take(SHOWN_WORDS)
                .copied()
                .collect::<Vec<_>>()
                .join("・"),
        });
        if out.len() == TOP {
            break;
        }
    }
    Ok(out)
}
