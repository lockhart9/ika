use std::env;
use std::path::Path;
use std::process::Command;

use anyhow::{Result, bail};

use crate::plugin::{self, Bump};

#[derive(clap::Args)]
pub struct Args {
    /// 実行するコマンドを表示するだけで、実行しない
    #[arg(long)]
    dry_run: bool,
    /// 2つのマニフェストの version を上げてから入れる。スキルを直したら必要
    /// （version が同じだと、Claude Code・Codex はインストール済みのコピーを更新しない）
    #[arg(long, value_enum)]
    bump: Option<Bump>,
}

/// このリポジトリを手元のプラグイン置き場（Claude・Codex の言う「マーケットプレイス」。公開ストアではない）として
/// 各クライアントの設定にパスを登録し、ika プラグインを入れる（入っていれば更新する）。
/// ネットワークへの送信や公開はしない。
/// プラグインはインストール時にキャッシュへコピーされ、version が同じだと更新されないので、
/// 直したときは `--bump` で version を上げる。
pub fn run(root: &Path, args: Args) -> Result<()> {
    match args.bump {
        Some(level) => {
            let (old, new) = plugin::bump(root, level, args.dry_run)?;
            let note = if args.dry_run {
                "（dry-run のため書き換えません）"
            } else {
                ""
            };
            println!("version: {old} -> {new}{note}");
        }
        None => {
            if let Some((version, files)) = plugin::changed_without_bump(root) {
                println!(
                    "注意: version {version} のときから {} ファイルが変わっています。version が同じだと、\
                     インストール済みのプラグインは更新されません（`ika install --bump patch`）",
                    files.len()
                );
            }
        }
    }

    let root_arg = root.display().to_string();
    let installed = plugin::installed_dir().is_some();
    let claude_step: Vec<&str> = if installed {
        vec!["plugin", "update", "ika@ika"]
    } else {
        vec!["plugin", "install", "ika@ika"]
    };
    let steps: Vec<(&str, Vec<&str>)> = vec![
        ("claude", vec!["plugin", "marketplace", "add", &root_arg]),
        ("claude", vec!["plugin", "marketplace", "update", "ika"]),
        ("claude", claude_step),
        ("codex", vec!["plugin", "marketplace", "add", &root_arg]),
    ];
    for (bin, bin_args) in &steps {
        let shown = format!("{bin} {}", bin_args.join(" "));
        if !on_path(bin) {
            println!("スキップ {shown}（{bin} が見つかりません）");
            continue;
        }
        println!("$ {shown}");
        if args.dry_run {
            continue;
        }
        let status = Command::new(bin).args(bin_args).status()?;
        if !status.success() {
            println!("  失敗しました（登録済みなら問題ありません）");
        }
    }

    let verified = if on_path("claude") && !args.dry_run {
        verify(root)?
    } else {
        true
    };
    println!(
        "\nCodex・ChatGPT デスクトップでは、Plugins 画面で ika を有効にするか、\
         ~/.codex/config.toml に次を追加してください:\n\n[plugins.\"ika@ika\"]\nenabled = true\n"
    );
    println!(
        "KB のデータは ika のリポジトリとは別（IKA_HOME）に置きます。未作成なら `ika init` で作り、"
    );
    println!("シェルの設定に export IKA_HOME=<KB を置くディレクトリ> を足してください");
    if !verified {
        bail!("Claude Code のプラグインを更新できませんでした");
    }
    Ok(())
}

/// インストール済みのコピーが、リポジトリと同じ内容になったか。同じなら `true`
fn verify(root: &Path) -> Result<bool> {
    let Some(installed) = plugin::installed_dir() else {
        println!(
            "\nClaude Code にはまだインストールされていません（上のコマンドの出力を確認してください）"
        );
        return Ok(false);
    };
    let version = plugin::version(root)?;
    let files = plugin::differs_from_installed(root, &installed)?;
    if files.is_empty() {
        println!(
            "\nClaude Code のプラグインは、リポジトリと同じ内容です（version {version}）。反映には Claude Code の再起動が必要です"
        );
        return Ok(true);
    }
    println!(
        "\n注意: Claude Code のプラグインはまだ古いコピーです（{} ファイルが違います: {}）。\n\
         version が同じだと更新されません。`ika install --bump patch` で version を上げてください",
        files.len(),
        plugin::describe_difference(&files)
    );
    Ok(false)
}

fn on_path(bin: &str) -> bool {
    env::var_os("PATH")
        .is_some_and(|paths| env::split_paths(&paths).any(|dir| dir.join(bin).is_file()))
}
