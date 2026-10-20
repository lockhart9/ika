use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::registry;
use crate::root::expand_tilde;
use crate::vcs;

#[derive(clap::Args)]
pub struct Args {
    /// 既存の ika リポジトリから kb/ と data/*.csv を取り込む（リポジトリ管理からの移行用）
    #[arg(long)]
    import: Option<String>,
}

const IGNORE: &str = ".DS_Store\n*.ika-tmp-*\n";

/// KB のデータディレクトリ（`IKA_HOME`）を作り、手元だけの git リポジトリにする
pub fn run(home: &Path, args: Args) -> Result<()> {
    if home.join(registry::PROJECTS).exists() {
        bail!("すでに初期化されています: {}", home.display());
    }
    fs::create_dir_all(home.join("data"))?;
    fs::create_dir_all(home.join("kb/projects"))?;

    match args.import {
        Some(source) => {
            let source = expand_tilde(&source);
            import(&source, home)?;
        }
        None => {
            fs::write(
                home.join(registry::PROJECTS),
                "name,path,url,status,summary\n",
            )?;
            fs::write(home.join(registry::DOMAINS), "name,summary\n")?;
        }
    }
    fs::write(home.join(".gitignore"), IGNORE)?;

    println!("作りました: {}", home.display());
    if let Err(err) = vcs::init(home) {
        println!("  履歴（git）は作れませんでした: {err:#}");
    } else {
        println!(
            "  手元だけの git リポジトリです（リモートなし）。書き込みごとに自動でコミットします"
        );
    }
    Ok(())
}

/// 移行元の kb/（書式の README を除く）と登録簿をコピーする
fn import(source: &Path, home: &Path) -> Result<()> {
    for file in [registry::PROJECTS, registry::DOMAINS] {
        fs::copy(source.join(file), home.join(file))
            .with_context(|| format!("{} を読めません", source.join(file).display()))?;
    }
    let mut files = 0;
    copy_tree(&source.join("kb"), &home.join("kb"), &mut files)?;
    let readme = home.join("kb/README.md");
    if readme.exists() {
        fs::remove_file(readme)?;
    }
    println!(
        "取り込みました: {} （kb/ のファイル {files} 件と登録簿）",
        source.display()
    );
    Ok(())
}

/// ディレクトリを丸ごとコピーする。シンボリックリンクと一時ファイルは含めない
fn copy_tree(from: &Path, to: &Path, files: &mut usize) -> Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from).with_context(|| format!("{} を読めません", from.display()))?
    {
        let entry = entry?;
        let kind = entry.file_type()?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if kind.is_symlink() || name_str == ".DS_Store" || name_str.contains(".ika-tmp-") {
            continue;
        }
        let dest = to.join(&name);
        if kind.is_dir() {
            copy_tree(&entry.path(), &dest, files)?;
        } else {
            fs::copy(entry.path(), dest)?;
            *files += 1;
        }
    }
    Ok(())
}
