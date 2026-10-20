//! `ika kb orphans`: どの insight・decision・プロダクトの資料からもリンクされていないノート。
//! プロダクトの README の「関連」は索引（一覧）なので、整理した証拠としては数えない。
//! 蒸留（insight への整理）の候補を、プロダクトごとに数える。

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::{check, frontmatter, kb};

#[derive(clap::Args)]
pub struct Args {
    /// このプロダクトのノートだけ
    #[arg(short, long)]
    project: Option<String>,
    /// この件数以上たまったプロダクトに「要整理」を付ける
    #[arg(long, default_value_t = 3)]
    min: usize,
}

struct Note {
    rel: PathBuf,
    title: String,
    date: String,
}

pub fn run(root: &Path, args: Args) -> Result<()> {
    let mut files = Vec::new();
    check::collect_markdown(&root.join("kb"), &mut files)?;
    files.sort();

    // 整理済みの側（insight・decision・kb/projects/ の README 以外の資料）が張っているリンク先
    let mut linked: HashSet<PathBuf> = HashSet::new();
    let mut notes: Vec<(Note, Vec<String>)> = Vec::new();
    for file in files {
        let rel = file.strip_prefix(root).unwrap_or(&file).to_path_buf();
        let text = fs::read_to_string(&file)?;
        let fields = frontmatter::parse(&text).ok().flatten().unwrap_or_default();
        let field = |key: &str| fields.get(key).cloned().unwrap_or_default();
        let parts: Vec<_> = rel.iter().map(|p| p.to_string_lossy()).collect();
        // README は「関連」にノートを並べる索引。設計資料などの README 以外は、ノートを根拠に書き起こした整理済みの資料
        let project_doc = parts.get(1).is_some_and(|p| p == "projects")
            && rel.file_name().is_some_and(|name| name != "README.md");

        let kind = field("type");
        if matches!(kind.as_str(), "insight" | "decision") || project_doc {
            let dir = rel.parent().unwrap_or(Path::new(""));
            for target in check::link_targets(&text) {
                linked.insert(kb::normalize(&dir.join(target)));
            }
        } else if kind == "note" && parts.get(2).is_some_and(|p| p == "notes") {
            let projects = fields
                .get("projects")
                .and_then(|v| frontmatter::list(v).ok())
                .unwrap_or_default();
            let title = Some(field("title"))
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| rel.display().to_string());
            notes.push((
                Note {
                    rel,
                    title,
                    date: field("date"),
                },
                projects,
            ));
        }
    }

    let mut groups: BTreeMap<String, Vec<&Note>> = BTreeMap::new();
    for (note, projects) in &notes {
        if linked.contains(&note.rel) {
            continue;
        }
        let names = if projects.is_empty() {
            vec!["（プロダクトなし）".to_string()]
        } else {
            projects.clone()
        };
        for name in names {
            if args.project.as_ref().is_none_or(|p| *p == name) {
                groups.entry(name).or_default().push(note);
            }
        }
    }
    if groups.is_empty() {
        println!("未整理のノートはありません");
        return Ok(());
    }

    println!(
        "未整理のノート（insight・decision・プロダクトの資料（README を除く）からリンクされていないもの）"
    );
    let mut groups: Vec<_> = groups.into_iter().collect();
    groups.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(&b.0)));
    for (name, mut notes) in groups {
        notes.sort_by(|a, b| (&b.date, &a.rel).cmp(&(&a.date, &b.rel)));
        let mark = if notes.len() >= args.min {
            format!("  ← 要整理（{} 件以上）", args.min)
        } else {
            String::new()
        };
        println!("\n{name}  {} 件{mark}", notes.len());
        for note in notes {
            println!(
                "  {}  {}\n      {}",
                note.date,
                note.title,
                note.rel.display()
            );
        }
    }
    Ok(())
}
