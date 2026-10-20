//! plugins/ika のプラグインとスキル。
//! スキルは Agent Skills 仕様（https://agentskills.io/specification）、
//! プラグインは Claude Code（.claude-plugin/plugin.json）と OpenAI（plugin.json）の両方の形式に沿わせる。

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::Value;

use crate::{check, frontmatter, registry};

pub const PLUGIN_DIR: &str = "plugins/ika";
const PLUGIN_NAME: &str = "ika";
/// Claude Code・claude.ai が読むマニフェストと、OpenAI（Codex・ChatGPT）が読むマニフェスト
pub const MANIFESTS: &[&str] = &[
    "plugins/ika/.claude-plugin/plugin.json",
    "plugins/ika/plugin.json",
];
const MARKETPLACES: &[&str] = &[
    ".claude-plugin/marketplace.json",
    ".agents/plugins/marketplace.json",
];

/// 正本からスキルへ配るファイル（正本, スキル内のパス, 配り先。None は全スキル）。
/// インストール時にプラグイン外へのシンボリックリンクは辿られないため、実ファイルとしてコピーする
const SHARED: &[(&str, &str, Option<&[&str]>)] = &[
    ("docs/kb-format.md", "references/kb-format.md", None),
    ("templates/note.md", "assets/note.md", Some(&["capture"])),
    (
        "templates/insight.md",
        "assets/insight.md",
        Some(&["capture"]),
    ),
    (
        "templates/decision.md",
        "assets/decision.md",
        Some(&["decide"]),
    ),
    ("templates/spec.md", "assets/spec.md", Some(&["cycle"])),
    (
        "templates/project.md",
        "assets/project.md",
        Some(&["project-add"]),
    ),
];

const MAX_NAME: usize = 64;
const MAX_DESCRIPTION: usize = 1024;
const MAX_COMPATIBILITY: usize = 500;
const MAX_LINES: usize = 500;

/// plugins/ika/skills 直下で SKILL.md を持つディレクトリ
pub fn dirs(root: &Path) -> Result<Vec<PathBuf>> {
    let mut dirs: Vec<_> = fs::read_dir(root.join(PLUGIN_DIR).join("skills"))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("SKILL.md").is_file())
        .collect();
    dirs.sort();
    Ok(dirs)
}

/// (正本, 配り先) の組
fn shared_copies(root: &Path) -> Result<Vec<(PathBuf, PathBuf)>> {
    let dirs = dirs(root)?;
    let mut copies = Vec::new();
    for (source, dest, targets) in SHARED {
        for dir in &dirs {
            let name = dir.file_name().expect("skills/<name>").to_string_lossy();
            if targets.is_none_or(|t| t.contains(&name.as_ref())) {
                copies.push((root.join(source), dir.join(dest)));
            }
        }
    }
    Ok(copies)
}

/// 正本（docs/kb-format.md・templates/）をスキルの references/・assets/ にコピーする
pub fn sync(root: &Path) -> Result<()> {
    let mut changed = 0;
    for (source, dest) in shared_copies(root)? {
        let content =
            fs::read(&source).with_context(|| format!("{} を読めません", source.display()))?;
        if fs::read(&dest).ok().as_ref() == Some(&content) {
            continue;
        }
        fs::create_dir_all(dest.parent().expect("skills/<name>/<dir>"))?;
        fs::write(&dest, content)?;
        println!(
            "更新  {}",
            dest.strip_prefix(root).unwrap_or(&dest).display()
        );
        changed += 1;
    }
    if changed == 0 {
        println!("変更なし");
    }
    Ok(())
}

pub fn check(root: &Path) -> Result<Vec<String>> {
    let mut problems = Vec::new();
    for dir in dirs(root)? {
        let name = dir.file_name().expect("skills/<name>").to_string_lossy();
        let rel = Path::new(PLUGIN_DIR)
            .join("skills")
            .join(name.as_ref())
            .join("SKILL.md");
        let text = fs::read_to_string(dir.join("SKILL.md"))?;
        for msg in check_skill(root, &rel, &name, &text) {
            problems.push(format!("{}: {msg}", rel.display()));
        }
    }
    for (source, dest) in shared_copies(root)? {
        if fs::read(&source).ok() != fs::read(&dest).ok() {
            problems.push(format!(
                "{}: {} と内容が違います（ika sync で更新してください）",
                dest.strip_prefix(root).unwrap_or(&dest).display(),
                source.strip_prefix(root).unwrap_or(&source).display()
            ));
        }
    }
    find_symlinks(root, &root.join(PLUGIN_DIR), &mut problems)?;
    problems.extend(check_manifests(root));
    problems.extend(crate::plugin::check(root));
    Ok(problems)
}

/// プラグインはどの環境でもそのまま動くよう、シンボリックリンクを含めない
fn find_symlinks(root: &Path, dir: &Path, problems: &mut Vec<String>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .display()
            .to_string();
        if path.is_symlink() {
            problems.push(format!("{rel}: プラグイン内にシンボリックリンクは置けません。実ファイルにしてください（正本の共有は ika sync で）"));
        } else if path.is_dir() {
            find_symlinks(root, &path, problems)?;
        }
    }
    Ok(())
}

fn read_json(root: &Path, file: &str) -> Result<Value, String> {
    let text = fs::read_to_string(root.join(file)).map_err(|_| format!("{file} がありません"))?;
    serde_json::from_str(&text).map_err(|err| format!("{file}: JSON が不正です: {err}"))
}

/// 2つのマニフェストと2つのマーケットプレイスが同じプラグインを指しているか
fn check_manifests(root: &Path) -> Vec<String> {
    let mut problems = Vec::new();
    let mut descriptions = Vec::new();
    for file in MANIFESTS {
        match read_json(root, file) {
            Ok(json) => {
                if json["name"] != PLUGIN_NAME {
                    problems.push(format!("{file}: name は {PLUGIN_NAME} にしてください"));
                }
                descriptions.push((json["version"].clone(), json["description"].clone()));
            }
            Err(err) => problems.push(err),
        }
    }
    if descriptions.len() == 2 && descriptions[0] != descriptions[1] {
        problems.push(format!(
            "{} と {} の version・description を揃えてください",
            MANIFESTS[0], MANIFESTS[1]
        ));
    }
    for file in MARKETPLACES {
        let json = match read_json(root, file) {
            Ok(json) => json,
            Err(err) => {
                problems.push(err);
                continue;
            }
        };
        let entry = json["plugins"]
            .as_array()
            .and_then(|plugins| plugins.iter().find(|p| p["name"] == PLUGIN_NAME));
        let Some(entry) = entry else {
            problems.push(format!("{file}: plugins に {PLUGIN_NAME} がありません"));
            continue;
        };
        // Claude は "./plugins/ika"、OpenAI は { "source": "local", "path": "./plugins/ika" }
        let source = entry["source"]
            .as_str()
            .or(entry["source"]["path"].as_str());
        if source.map(|s| s.trim_start_matches("./").trim_end_matches('/')) != Some(PLUGIN_DIR) {
            problems.push(format!(
                "{file}: {PLUGIN_NAME} の source は ./{PLUGIN_DIR} にしてください"
            ));
        }
    }
    problems
}

fn check_skill(root: &Path, rel: &Path, dir_name: &str, text: &str) -> Vec<String> {
    let fields = match frontmatter::parse(text) {
        Ok(Some(fields)) => fields,
        Ok(None) => return vec!["フロントマターがありません".into()],
        Err(err) => return vec![err.to_string()],
    };
    let mut problems = Vec::new();
    let len = |key: &str| fields.get(key).map(|v| v.chars().count());

    match fields.get("name") {
        None => problems.push("name がありません".into()),
        Some(name) => {
            if name != dir_name {
                problems.push(format!("name はディレクトリ名と同じにしてください: {name}"));
            }
            if !registry::is_valid_name(name) || name.contains("--") || name.len() > MAX_NAME {
                problems.push(format!(
                    "name は {MAX_NAME} 字以内の英小文字・数字・ハイフンにしてください（連続ハイフン不可）"
                ));
            }
        }
    }
    match len("description") {
        None | Some(0) => problems.push("description がありません".into()),
        Some(n) if n > MAX_DESCRIPTION => problems.push(format!(
            "description が長すぎます（{n} / {MAX_DESCRIPTION} 字）"
        )),
        _ => {}
    }
    if let Some(n) = len("compatibility").filter(|n| *n > MAX_COMPATIBILITY) {
        problems.push(format!(
            "compatibility が長すぎます（{n} / {MAX_COMPATIBILITY} 字）"
        ));
    }
    let lines = text.lines().count();
    if lines >= MAX_LINES {
        problems.push(format!(
            "SKILL.md が長すぎます（{lines} 行）。詳細は references/ に分けてください"
        ));
    }
    problems.extend(check::check_links(root, rel, text));
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_mismatched_or_invalid_names() {
        let root = Path::new("/nonexistent");
        let rel = Path::new("plugins/ika/skills/a/SKILL.md");
        let ok = "---\nname: a\ndescription: x\n---\n";
        assert!(check_skill(root, rel, "a", ok).is_empty());
        let bad = "---\nname: a--b\ndescription:\n---\n";
        let problems = check_skill(root, rel, "a", bad);
        assert_eq!(problems.len(), 3, "{problems:?}");
    }
}
