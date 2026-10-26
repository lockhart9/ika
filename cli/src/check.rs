use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Result;
use chrono::NaiveDate;

use crate::frontmatter;
use crate::registry;
use crate::skills;

const REQUIRED: &[&str] = &["title", "type", "date", "projects", "tags"];
const DECISION_STATUSES: &[&str] = &["proposed", "accepted", "superseded", "rejected"];
pub const SPEC_STATUSES: &[&str] = &["draft", "approved", "done", "dropped"];

/// `root` は KB のデータ（IKA_HOME）。`repo` が分かれば、プラグインの検証もする
pub fn run(root: &Path, repo: Option<&Path>) -> Result<ExitCode> {
    let mut problems = Vec::new();

    let projects = registry::projects(root)?;
    let mut names = HashSet::new();
    for p in &projects {
        let mut report =
            |msg: String| problems.push(format!("{}: {}: {msg}", registry::PROJECTS, p.name));
        if !names.insert(p.name.as_str()) {
            report("プロダクト名が重複しています".into());
        }
        if !registry::is_valid_name(&p.name) {
            report("プロダクト名は英小文字・数字・ハイフンにしてください".into());
        }
        if !registry::STATUSES.contains(&p.status.as_str()) {
            report(format!("status が不正です: {}", p.status));
        }
        if !registry::readme_path(root, &p.name).is_file() {
            report(format!("kb/projects/{}/README.md がありません", p.name));
        }
    }

    let domains = registry::domains(root)?;
    let mut domain_names = HashSet::new();
    for d in &domains {
        let mut report =
            |msg: String| problems.push(format!("{}: {}: {msg}", registry::DOMAINS, d.name));
        if !domain_names.insert(d.name.as_str()) {
            report("領域名が重複しています".into());
        }
        if !registry::is_valid_name(&d.name) || registry::RESERVED.contains(&d.name.as_str()) {
            report("領域名が不正です".into());
        }
        if !root.join("kb").join(&d.name).is_dir() {
            report(format!("kb/{}/ がありません", d.name));
        }
    }
    for entry in fs::read_dir(root.join("kb"))? {
        let path = entry?.path();
        let name = path.file_name().expect("entry").to_string_lossy();
        if path.is_dir() && name != "projects" && !domain_names.contains(name.as_ref()) {
            problems.push(format!(
                "kb/{name}/: 未登録の領域です（`ika domain add {name}` で登録してください）"
            ));
        }
    }

    problems.extend(crate::data::check(root)?);

    let mut files = Vec::new();
    collect_markdown(&root.join("kb"), &mut files)?;
    files.sort();
    for file in &files {
        let rel = file.strip_prefix(root).unwrap_or(file);
        for msg in check_file(root, rel, &fs::read_to_string(file)?, &names, &domain_names) {
            problems.push(format!("{}: {msg}", rel.display()));
        }
    }

    match repo {
        Some(repo) => {
            problems.extend(skills::check(repo)?);
            if let Some(note) = crate::plugin::installed_note(repo) {
                println!("{note}");
            }
        }
        None => println!("（ika のリポジトリの外なので、プラグインの検証は省きました）"),
    }

    let notices = unlinked_recent_notes(root, &files)?;
    if problems.is_empty() {
        println!(
            "OK（{} プロダクト、{} 領域、{} ファイル）",
            projects.len(),
            domains.len(),
            files.len()
        );
        print_notices(&notices);
        return Ok(ExitCode::SUCCESS);
    }
    for problem in &problems {
        println!("{problem}");
    }
    println!("\n{} 件の問題があります", problems.len());
    print_notices(&notices);
    Ok(ExitCode::FAILURE)
}

/// 「新しい」とみなす日数（`unlinked_recent_notes`）
const RECENT_DAYS: u64 = 14;

/// 作成日が最近で、kb/ のどのファイルからもリンクされていないノート（日付, パス）。古い孤立は `ika kb triage` が受け持つ
fn unlinked_recent_notes(root: &Path, files: &[PathBuf]) -> Result<Vec<(String, String)>> {
    let cutoff = chrono::Local::now()
        .date_naive()
        .checked_sub_days(chrono::Days::new(RECENT_DAYS))
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default();
    let mut linked: HashSet<PathBuf> = HashSet::new();
    let mut recent: Vec<(String, PathBuf)> = Vec::new();
    for file in files {
        let rel = file.strip_prefix(root).unwrap_or(file).to_path_buf();
        let text = fs::read_to_string(file)?;
        let dir = rel.parent().unwrap_or(Path::new(""));
        for target in link_targets(&text) {
            let target = crate::kb::normalize(&dir.join(target));
            if target != rel {
                linked.insert(target);
            }
        }
        let fields = frontmatter::parse(&text).ok().flatten().unwrap_or_default();
        let date = fields.get("date").cloned().unwrap_or_default();
        let in_notes = rel.iter().nth(2).is_some_and(|p| p == "notes");
        if fields.get("type").is_some_and(|t| t == "note") && in_notes && date >= cutoff {
            recent.push((date, rel));
        }
    }
    recent.retain(|(_, rel)| !linked.contains(rel));
    recent.sort();
    Ok(recent
        .into_iter()
        .map(|(date, rel)| (date, rel.display().to_string()))
        .collect())
}

fn print_notices(notices: &[(String, String)]) {
    if notices.is_empty() {
        return;
    }
    println!(
        "\n注意（失敗にはしません）: {RECENT_DAYS} 日以内のノートのうち、どこからもリンクされていないもの {} 件",
        notices.len()
    );
    for (date, rel) in notices {
        println!("  {date}  {rel}");
    }
    println!("  insight・決定・プロダクトの README の「関連」からリンクを張ってください");
}

/// 1ファイルだけ検証する（`ika kb` の書き込み後に使う）。`rel` はルートからの相対パス
pub fn file(root: &Path, rel: &Path, text: &str) -> Result<Vec<String>> {
    let projects = registry::projects(root)?;
    let domains = registry::domains(root)?;
    let project_names = projects.iter().map(|p| p.name.as_str()).collect();
    let domain_names = domains.iter().map(|d| d.name.as_str()).collect();
    Ok(check_file(root, rel, text, &project_names, &domain_names))
}

pub fn collect_markdown(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_markdown(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "md") {
            out.push(path);
        }
    }
    Ok(())
}

fn check_file(
    root: &Path,
    rel: &Path,
    text: &str,
    projects: &HashSet<&str>,
    domains: &HashSet<&str>,
) -> Vec<String> {
    let fields = match frontmatter::parse(text) {
        Ok(Some(fields)) => fields,
        Ok(None) => return vec!["フロントマターがありません".into()],
        Err(err) => return vec![err.to_string()],
    };
    let mut problems = Vec::new();
    for key in REQUIRED {
        if fields
            .get(*key)
            .is_none_or(|v| v.is_empty() && !matches!(*key, "projects" | "tags"))
        {
            problems.push(format!("{key} がありません"));
        }
    }
    if let Some(date) = fields.get("date").filter(|d| !d.is_empty())
        && NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err()
    {
        problems.push(format!("date は YYYY-MM-DD で書いてください: {date}"));
    }
    let listed =
        |key: &str, problems: &mut Vec<String>| match fields.get(key).map(|v| frontmatter::list(v))
        {
            Some(Ok(items)) => items,
            Some(Err(err)) => {
                problems.push(format!("{key}: {err}"));
                Vec::new()
            }
            None => Vec::new(),
        };
    let file_projects = listed("projects", &mut problems);
    listed("tags", &mut problems);
    for project in &file_projects {
        if !projects.contains(project.as_str()) {
            problems.push(format!("未登録のプロダクトです: {project}"));
        }
    }

    problems.extend(check_location(rel, &fields, &file_projects, domains));
    // 取り込んだ資料のリンクは原本の置き場所を前提にしているので、KB の中では確かめない。
    // 図の記法も、取り込んだ資料では展開しないので確かめない
    if fields.get("type").is_none_or(|t| t != "source") {
        problems.extend(check_links(root, rel, text));
        let offset = frontmatter::head(text).matches('\n').count();
        for error in crate::render::diagram_errors(frontmatter::body(text)) {
            problems.push(format!("{}行目: {}", error.line + offset, error.message));
        }
    }
    problems
}

/// type と置き場所が対応しているか
fn check_location(
    rel: &Path,
    fields: &BTreeMap<String, String>,
    file_projects: &[String],
    domains: &HashSet<&str>,
) -> Vec<String> {
    let parts: Vec<_> = rel.iter().map(|p| p.to_string_lossy()).collect();
    let dir = parts.get(1).map(|s| s.as_ref()).unwrap_or("");
    let ty = fields.get("type").map(String::as_str).unwrap_or("");
    let mut problems = Vec::new();
    match ty {
        "project" => {
            let name = parts.get(2).map(|s| s.as_ref()).unwrap_or("");
            if dir != "projects" || parts.len() != 4 || parts[3] != "README.md" {
                problems.push("type: project は kb/projects/<name>/README.md に置きます".into());
            } else if file_projects != [name] {
                problems.push(format!("projects は [{name}] にしてください"));
            }
        }
        "spec" => {
            let name = parts.get(2).map(|s| s.as_ref()).unwrap_or("");
            if dir != "projects" || parts.len() != 5 || parts[3] != "specs" {
                problems.push("type: spec は kb/projects/<name>/specs/ に置きます".into());
            } else if file_projects != [name] {
                problems.push(format!("projects は [{name}] にしてください"));
            }
            let status = fields.get("status").map(String::as_str).unwrap_or("");
            if !SPEC_STATUSES.contains(&status) {
                problems.push(format!(
                    "status は {} のいずれかです",
                    SPEC_STATUSES.join(" / ")
                ));
            }
        }
        "source" => {
            let in_project = dir == "projects"
                && parts.len() == 5
                && parts[3] == "sources"
                && file_projects.iter().any(|p| p == parts[2].as_ref());
            let in_domain = domains.contains(dir) && parts.len() == 4 && parts[2] == "sources";
            if !in_project && !in_domain {
                problems.push(
                    "type: source は kb/projects/<name>/sources/ か kb/<領域>/sources/ に置きます（`ika import` で作る）"
                        .into(),
                );
            }
            if fields.get("source").is_none_or(|s| s.is_empty()) {
                problems.push("source（原本の URL かパス）がありません".into());
            }
        }
        "note" | "insight" | "decision" => {
            let expected = format!("{ty}s");
            let kind_dir = parts.get(2).map(|s| s.as_ref()).unwrap_or("");
            let in_domain = domains.contains(dir) && kind_dir == expected;
            if !in_domain && dir != "projects" {
                problems.push(format!("type: {ty} は kb/<領域>/{expected}/ に置きます"));
            }
            if ty == "decision" {
                let status = fields.get("status").map(String::as_str).unwrap_or("");
                if !DECISION_STATUSES.contains(&status) {
                    problems.push(format!(
                        "status は {} のいずれかです",
                        DECISION_STATUSES.join(" / ")
                    ));
                }
            }
        }
        "" => {}
        other => problems.push(format!("type が不正です: {other}")),
    }
    problems
}

/// 相対パスの Markdown リンクが実在するか
pub fn check_links(root: &Path, rel: &Path, text: &str) -> Vec<String> {
    let base = root.join(rel.parent().unwrap_or(Path::new("")));
    link_targets(text)
        .into_iter()
        .filter(|target| !base.join(target).exists())
        .map(|target| format!("リンク先がありません: {target}"))
        .collect()
}

/// 本文中の相対パスの Markdown リンク先（`#` 以降は除く。コードと外部URLは除く）
pub fn link_targets(text: &str) -> Vec<String> {
    let mut targets = Vec::new();
    map_links(text, |target| {
        targets.push(target.to_string());
        None
    });
    targets
}

/// 本文中の相対パスの Markdown リンクのパス部分を `change` で書き換える。
/// `change` が `None` を返したリンクはそのまま。`#` 以降・フロントマター・コード・外部URLには触れない
pub fn map_links(text: &str, mut change: impl FnMut(&str) -> Option<String>) -> String {
    let head = frontmatter::head(text);
    let mut out = String::from(head);
    let mut in_fence = false;
    for line in text[head.len()..].split_inclusive('\n') {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            out.push_str(line);
        } else if in_fence {
            out.push_str(line);
        } else {
            // 偶数番目がコードの外、奇数番目がインラインコード（`...`）の中
            let segments: Vec<String> = line
                .split('`')
                .enumerate()
                .map(|(n, seg)| {
                    if n % 2 == 0 {
                        map_segment(seg, &mut change)
                    } else {
                        seg.to_string()
                    }
                })
                .collect();
            out.push_str(&segments.join("`"));
        }
    }
    out
}

fn map_segment(segment: &str, change: &mut impl FnMut(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = segment;
    while let Some(start) = rest.find("](") {
        let (before, after) = rest.split_at(start + 2);
        out.push_str(before);
        let Some(end) = after.find(')') else {
            rest = after;
            break;
        };
        let target = &after[..end];
        let path_end = target.find('#').unwrap_or(target.len());
        let path = target[..path_end].trim();
        let skip = path.is_empty() || path.contains("://") || path.starts_with("mailto:");
        match (!skip).then(|| change(path)).flatten() {
            Some(new) => {
                out.push_str(&new);
                out.push_str(&target[path_end..]);
            }
            None => out.push_str(target),
        }
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_links_rewrites_only_plain_relative_links() {
        let text = "---\ntitle: [a](x.md)\n---\n本文 [a](old.md#sec) と `[b](old.md)` と [c](https://x.test/old.md)\n```\n[d](old.md)\n```\n[e](old.md)\n";
        let out = map_links(text, |t| (t == "old.md").then(|| "../new.md".to_string()));
        assert_eq!(
            out,
            "---\ntitle: [a](x.md)\n---\n本文 [a](../new.md#sec) と `[b](old.md)` と [c](https://x.test/old.md)\n```\n[d](old.md)\n```\n[e](../new.md)\n"
        );
    }

    #[test]
    fn link_targets_lists_the_same_links() {
        let text = "[a](a.md#x) `[b](b.md)` [c](https://x.test) [d](d.md)\n";
        assert_eq!(link_targets(text), ["a.md", "d.md"]);
    }
}
