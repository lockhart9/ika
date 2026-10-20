use std::env;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

/// KB のデータを置く場所。環境変数 `IKA_HOME` で指定する（未設定ならエラー。既定値はない）。
/// 中は `kb/`・`data/projects.csv`・`data/domains.csv`。ika のリポジトリとは別に持つ
pub fn data_path() -> Result<PathBuf> {
    match env::var("IKA_HOME") {
        Ok(path) if !path.is_empty() => Ok(expand_tilde(&path)),
        _ => bail!(
            "IKA_HOME が設定されていません。KB の場所を設定してください（例: export IKA_HOME=<KB を置くディレクトリ>）"
        ),
    }
}

/// [`data_path`] を返す。初期化されていなければエラーにする
pub fn data_dir() -> Result<PathBuf> {
    let path = data_path()?;
    if !path.join("data/projects.csv").is_file() {
        bail!(
            "KB がありません（IKA_HOME={}）。`ika init` で作ってください（既存のリポジトリから移すなら `ika init --import <リポジトリ>`）",
            path.display()
        );
    }
    Ok(path)
}

/// ika のリポジトリ（CLI・プラグイン・ひな形のソース）のルート。
/// `sync`・`export`・`install` などの開発用コマンドが、カレントから親へ遡って探す
pub fn repo() -> Result<PathBuf> {
    let mut dir = env::current_dir()?;
    loop {
        if is_repo(&dir) {
            return Ok(dir);
        }
        if !dir.pop() {
            bail!(
                "ika のリポジトリの中で実行してください（このコマンドはプラグイン・ひな形を扱います）"
            );
        }
    }
}

fn is_repo(path: &Path) -> bool {
    path.join("AGENTS.md").is_file() && path.join("plugins/ika").is_dir()
}

fn home_dir() -> Result<PathBuf> {
    match env::var_os("HOME") {
        Some(home) => Ok(PathBuf::from(home)),
        None => bail!("HOME が設定されていません"),
    }
}

/// 先頭の `~/` をホームディレクトリに展開する
pub fn expand_tilde(path: &str) -> PathBuf {
    match (path.strip_prefix("~/"), home_dir()) {
        (Some(rest), Ok(home)) => home.join(rest),
        _ => PathBuf::from(path),
    }
}
