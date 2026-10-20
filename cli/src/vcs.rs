//! KB のデータディレクトリを手元だけの git リポジトリとして履歴を残す。
//! リモートは設定しない。書き込みコマンドの最後に、溜まった変更を1コミットにまとめる。

use std::io;
use std::path::Path;
use std::process::{Command, Output};

use anyhow::{Result, bail};

fn git(dir: &Path, args: &[&str]) -> io::Result<Output> {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "commit.gpgsign=false", "-c", "core.quotepath=off"])
        .args(args)
        .output()
}

fn failed(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).trim().to_string()
}

/// `git init` して現状を最初のコミットにする
pub fn init(dir: &Path) -> Result<()> {
    let output = git(dir, &["init", "-q"])?;
    if !output.status.success() {
        bail!("git init に失敗しました: {}", failed(&output));
    }
    // 既定ブランチ名の設定に左右されないよう main に揃える
    git(dir, &["symbolic-ref", "HEAD", "refs/heads/main"])?;
    commit_all(dir, "ika init")
}

/// 変更をすべて1コミットにする。git のリポジトリでなければ何もしない。
/// 失敗しても書き込み自体は済んでいるので、警告だけ出して続ける
pub fn commit(dir: &Path, message: &str) {
    if !dir.join(".git").exists() {
        return;
    }
    if let Err(err) = commit_all(dir, message) {
        eprintln!("警告: 履歴への記録に失敗しました（書き込みは済んでいます）: {err:#}");
    }
}

fn commit_all(dir: &Path, message: &str) -> Result<()> {
    let add = git(dir, &["add", "-A"])?;
    if !add.status.success() {
        bail!("git add に失敗しました: {}", failed(&add));
    }
    // 差分があるときだけ終了コード 1
    let diff = git(dir, &["diff", "--cached", "--quiet"])?;
    if diff.status.code() != Some(1) {
        return Ok(());
    }
    let commit = git(dir, &["commit", "-q", "-m", message])?;
    if !commit.status.success() {
        bail!("git commit に失敗しました: {}", failed(&commit));
    }
    Ok(())
}
