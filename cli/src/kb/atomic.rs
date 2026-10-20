//! 並行セッションが同じファイルを触っても壊れないための書き込み。
//! 一時ファイルに書いてから置き換えるので、途中の状態は他から見えない。

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// 同じディレクトリに置く一時ファイル（拡張子が `.md` にならないので KB の走査に出ない）
fn temp_path(path: &Path) -> PathBuf {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    path.with_file_name(format!(".{name}.ika-tmp-{}", std::process::id()))
}

/// 一時ファイルに書いて `place` で本来の場所に移す。失敗しても一時ファイルは残さない
fn via_temp(path: &Path, text: &str, place: impl FnOnce(&Path, &Path) -> Result<()>) -> Result<()> {
    let temp = temp_path(path);
    let result = fs::write(&temp, text)
        .with_context(|| format!("書けません: {}", path.display()))
        .and_then(|()| place(&temp, path));
    if result.is_err() || temp.exists() {
        let _ = fs::remove_file(&temp);
    }
    result
}

/// 新規作成。すでにあれば何も書かずに失敗する（確認と作成の間に作られた場合も）
pub fn create_new(path: &Path, text: &str) -> Result<()> {
    via_temp(path, text, |temp, path| match fs::hard_link(temp, path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == ErrorKind::AlreadyExists => {
            bail!("すでに存在します: {}", path.display())
        }
        Err(err) => Err(err.into()),
    })
}

/// 内容を丸ごと置き換える
pub fn overwrite(path: &Path, text: &str) -> Result<()> {
    via_temp(path, text, |temp, path| Ok(fs::rename(temp, path)?))
}

/// 読んで `change` で書き換えて置き換える。読んでから置き換えるまでに
/// ほかのプロセスがファイルを変えていたら、上書きせずに失敗する
pub fn modify(path: &Path, change: impl FnOnce(&str) -> Result<String>) -> Result<String> {
    let original =
        fs::read_to_string(path).with_context(|| format!("読めません: {}", path.display()))?;
    let updated = change(&original)?;
    via_temp(path, &updated, |temp, path| {
        if fs::read_to_string(path).ok().as_deref() != Some(original.as_str()) {
            bail!(
                "編集中に別のプロセスがファイルを変更しました。読み直してやり直してください: {}",
                path.display()
            );
        }
        Ok(fs::rename(temp, path)?)
    })?;
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ika-atomic-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn leftovers(dir: &Path) -> Vec<String> {
        fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains("ika-tmp"))
            .collect()
    }

    #[test]
    fn create_new_refuses_existing_and_leaves_no_temp() {
        let dir = dir("create");
        let file = dir.join("a.md");
        create_new(&file, "one").unwrap();
        assert!(create_new(&file, "two").is_err());
        assert_eq!(fs::read_to_string(&file).unwrap(), "one");
        assert!(leftovers(&dir).is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn modify_replaces_content() {
        let dir = dir("modify");
        let file = dir.join("a.md");
        fs::write(&file, "a b").unwrap();
        let updated = modify(&file, |text| Ok(text.replace('b', "c"))).unwrap();
        assert_eq!(updated, "a c");
        assert_eq!(fs::read_to_string(&file).unwrap(), "a c");
        assert!(leftovers(&dir).is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn modify_fails_when_changed_concurrently() {
        let dir = dir("race");
        let file = dir.join("a.md");
        fs::write(&file, "original").unwrap();
        let result = modify(&file, |_| {
            // 読んだあとに別のプロセスが書き換えた状況
            fs::write(&file, "other session").unwrap();
            Ok("mine".to_string())
        });
        assert!(result.is_err());
        assert_eq!(fs::read_to_string(&file).unwrap(), "other session");
        assert!(leftovers(&dir).is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }
}
