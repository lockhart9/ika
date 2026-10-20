use std::fs;
use std::path::Path;

use anyhow::{Result, bail};
use clap::Subcommand;

use crate::registry::{self, Domain};
use crate::vcs;

#[derive(Subcommand)]
pub enum Command {
    /// 登録済みの領域を一覧する
    List,
    /// 領域を登録し、kb/<name>/ に notes・insights・decisions を作る
    Add {
        /// 英小文字・数字・ハイフン
        name: String,
        /// 何を置く領域か
        #[arg(long, default_value = "")]
        summary: String,
    },
}

pub fn run(root: &Path, command: Command) -> Result<()> {
    match command {
        Command::List => {
            for d in registry::domains(root)? {
                println!("{:<16} {}", d.name, d.summary);
            }
            Ok(())
        }
        Command::Add { name, summary } => add(root, &Domain { name, summary }),
    }
}

fn add(root: &Path, domain: &Domain) -> Result<()> {
    if !registry::is_valid_name(&domain.name) || registry::RESERVED.contains(&domain.name.as_str())
    {
        bail!(
            "領域名は英小文字・数字・ハイフンにし、{} 以外にしてください: {}",
            registry::RESERVED.join(" / "),
            domain.name
        );
    }
    if registry::domains(root)?
        .iter()
        .any(|d| d.name == domain.name)
    {
        bail!("すでに登録されています: {}", domain.name);
    }

    for kind in registry::KIND_DIRS {
        let dir = root.join("kb").join(&domain.name).join(kind);
        fs::create_dir_all(&dir)?;
        let keep = dir.join(".gitkeep");
        if !keep.exists() {
            fs::write(keep, "")?;
        }
    }
    registry::append(root, registry::DOMAINS, domain)?;

    vcs::commit(root, &format!("domain add {}", domain.name));
    println!("登録しました: {}", domain.name);
    println!("  {}", registry::DOMAINS);
    println!("  kb/{}/{{{}}}", domain.name, registry::KIND_DIRS.join(","));
    Ok(())
}
