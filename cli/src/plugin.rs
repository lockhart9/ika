//! ika プラグインの version と、インストール済みのコピーとの差。
//! Claude Code・Codex はインストール時にプラグインをキャッシュへコピーし、version が同じだと
//! 更新しない。スキルを直したのに version が同じ、という状態を `ika check` で見つけ、
//! `ika install --bump` で上げて入れ直す。

use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use serde_json::Value;

use crate::skills::{MANIFESTS, PLUGIN_DIR};

#[derive(Clone, Copy, ValueEnum)]
pub enum Bump {
    Patch,
    Minor,
    Major,
}

/// `1.2.3` を上げる。`major` なら `2.0.0`、`minor` なら `1.3.0`、`patch` なら `1.2.4`
pub fn bumped(version: &str, level: Bump) -> Result<String> {
    let parts: Vec<u64> = version
        .split('.')
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()
        .filter(|p: &Vec<u64>| p.len() == 3)
        .with_context(|| format!("version は x.y.z の形にしてください: {version}"))?;
    let [major, minor, patch] = [parts[0], parts[1], parts[2]];
    Ok(match level {
        Bump::Major => format!("{}.0.0", major + 1),
        Bump::Minor => format!("{major}.{}.0", minor + 1),
        Bump::Patch => format!("{major}.{minor}.{}", patch + 1),
    })
}

/// 現在の version（Claude 用マニフェストのもの）
pub fn version(root: &Path) -> Result<String> {
    manifest_version(&fs::read_to_string(root.join(MANIFESTS[0]))?)
        .with_context(|| format!("{} に version がありません", MANIFESTS[0]))
}

fn manifest_version(text: &str) -> Option<String> {
    serde_json::from_str::<Value>(text)
        .ok()?
        .get("version")?
        .as_str()
        .map(String::from)
}

/// `"version": "<旧>"` の値だけを差し替える（ほかの書式は崩さない）
fn set_version(text: &str, new: &str) -> Option<String> {
    let key = text.find("\"version\"")?;
    let colon = key + text[key..].find(':')?;
    let open = colon + text[colon..].find('"')?;
    let close = open + 1 + text[open + 1..].find('"')?;
    Some(format!("{}{new}{}", &text[..=open], &text[close..]))
}

/// 2つのマニフェストの version を上げる。`dry_run` なら書かない。戻り値は (旧, 新)
pub fn bump(root: &Path, level: Bump, dry_run: bool) -> Result<(String, String)> {
    let old = version(root)?;
    let new = bumped(&old, level)?;
    for file in MANIFESTS {
        let path = root.join(file);
        let text = fs::read_to_string(&path).with_context(|| format!("{file} を読めません"))?;
        let Some(updated) = set_version(&text, &new) else {
            bail!("{file} に version がありません");
        };
        if !dry_run {
            fs::write(&path, updated)?;
        }
    }
    Ok((old, new))
}

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// 最後に version を変えたコミットのときから、`plugins/ika` の中身が変わっているのに
/// version が同じなら、(その version, 変わったファイル) を返す。
/// 未コミットの編集と新規ファイルも含む。git が無い・履歴が無いときは `None`
pub fn changed_without_bump(root: &Path) -> Option<(String, Vec<String>)> {
    let manifests: Vec<&str> = MANIFESTS.to_vec();
    let mut log = vec!["log", "-1", "--format=%H", "-G\"version\"", "--"];
    log.extend(&manifests);
    let base = git(root, &log).filter(|h| !h.is_empty())?;

    let at_base = git(root, &["show", &format!("{base}:{}", MANIFESTS[0])])?;
    let base_version = manifest_version(&at_base)?;
    if version(root).ok()? != base_version {
        return None; // すでに上げてある
    }

    let mut changed: BTreeSet<String> = BTreeSet::new();
    for args in [
        vec!["diff", "--name-only", base.as_str(), "--", PLUGIN_DIR],
        vec![
            "ls-files",
            "--others",
            "--exclude-standard",
            "--",
            PLUGIN_DIR,
        ],
    ] {
        changed.extend(git(root, &args)?.lines().map(String::from));
    }
    changed.retain(|f| !f.is_empty());
    (!changed.is_empty()).then(|| (base_version, changed.into_iter().collect()))
}

/// `ika check` に出す問題
pub fn check(root: &Path) -> Vec<String> {
    let Some((version, files)) = changed_without_bump(root) else {
        return Vec::new();
    };
    vec![format!(
        "{PLUGIN_DIR}: version {version} のときから {} ファイルが変わっています（{}）。\
         version を上げてください（`ika install --bump patch`）。上げないと、インストール済みのプラグインは更新されません",
        files.len(),
        summary(&files)
    )]
}

fn summary(files: &[String]) -> String {
    let shown: Vec<_> = files
        .iter()
        .take(4)
        .map(|f| f.strip_prefix("plugins/ika/").unwrap_or(f).to_string())
        .collect();
    let more = files.len().saturating_sub(shown.len());
    if more == 0 {
        shown.join(", ")
    } else {
        format!("{}, ほか {more} 件", shown.join(", "))
    }
}

/// Claude Code の設定ディレクトリ（`CLAUDE_CONFIG_DIR` か `~/.claude`）
fn claude_dir() -> Option<PathBuf> {
    match env::var_os("CLAUDE_CONFIG_DIR") {
        Some(dir) if !dir.is_empty() => Some(PathBuf::from(dir)),
        _ => env::var_os("HOME").map(|home| PathBuf::from(home).join(".claude")),
    }
}

/// Claude Code にインストール済みの ika プラグインのコピー（無ければ `None`）
pub fn installed_dir() -> Option<PathBuf> {
    let text = fs::read_to_string(claude_dir()?.join("plugins/installed_plugins.json")).ok()?;
    let json: Value = serde_json::from_str(&text).ok()?;
    let plugins = json.get("plugins").unwrap_or(&json);
    let path = plugins
        .get("ika@ika")?
        .get(0)?
        .get("installPath")?
        .as_str()?;
    let dir = PathBuf::from(path);
    dir.is_dir().then_some(dir)
}

/// リポジトリの `plugins/ika` とインストール済みのコピーで、内容が違うファイル（相対パス）
pub fn differs_from_installed(root: &Path, installed: &Path) -> Result<Vec<String>> {
    fn files(dir: &Path, base: &Path, out: &mut BTreeSet<String>) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            if path.is_dir() {
                files(&path, base, out)?;
            } else {
                out.insert(path.strip_prefix(base)?.to_string_lossy().into_owned());
            }
        }
        Ok(())
    }
    let repo = root.join(PLUGIN_DIR);
    let (mut a, mut b) = (BTreeSet::new(), BTreeSet::new());
    files(&repo, &repo, &mut a)?;
    files(installed, installed, &mut b)?;
    Ok(a.union(&b)
        .filter(|f| fs::read(repo.join(f)).ok() != fs::read(installed.join(f)).ok())
        .cloned()
        .collect())
}

/// `ika check` の補足: インストール済みのコピーが古ければ一言添える（エラーにはしない）
pub fn installed_note(root: &Path) -> Option<String> {
    let installed = installed_dir()?;
    let files = differs_from_installed(root, &installed).ok()?;
    (!files.is_empty()).then(|| {
        format!(
            "（インストール済みの ika プラグインは古いコピーです: {} ファイルが違います。`ika install` で更新してください）",
            files.len()
        )
    })
}

pub fn describe_difference(files: &[String]) -> String {
    summary(
        &files
            .iter()
            .map(|f| format!("plugins/ika/{f}"))
            .collect::<Vec<_>>(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bumped_raises_the_requested_part() {
        assert_eq!(bumped("0.1.9", Bump::Patch).unwrap(), "0.1.10");
        assert_eq!(bumped("0.1.9", Bump::Minor).unwrap(), "0.2.0");
        assert_eq!(bumped("1.4.2", Bump::Major).unwrap(), "2.0.0");
        assert!(bumped("1.2", Bump::Patch).is_err());
        assert!(bumped("a.b.c", Bump::Patch).is_err());
    }

    #[test]
    fn set_version_only_replaces_the_value() {
        let text =
            "{\n  \"name\": \"ika\",\n  \"version\":   \"0.1.0\",\n  \"description\": \"d\"\n}\n";
        assert_eq!(
            set_version(text, "0.2.0").unwrap(),
            "{\n  \"name\": \"ika\",\n  \"version\":   \"0.2.0\",\n  \"description\": \"d\"\n}\n"
        );
        assert!(set_version("{}", "1.0.0").is_none());
        assert_eq!(manifest_version(text).unwrap(), "0.1.0");
    }
}
