#![forbid(unsafe_code)]

// マクロ（println!・print!）を使う側のモジュールより先に置く
#[macro_use]
mod out;

mod check;
mod data;
mod domain;
mod export;
mod frontmatter;
mod import;
mod init;
mod install;
mod kb;
mod new;
mod plugin;
mod project;
mod registry;
mod render;
mod root;
mod serve;
mod skills;
mod templates;
mod vcs;

use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// 個人ナレッジハーネス ika の CLI
#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// KB のデータディレクトリ（IKA_HOME）を作る。手元だけの git で履歴を残す
    Init(init::Args),
    /// プロダクト登録簿（data/projects.csv）を操作する
    Project {
        #[command(subcommand)]
        command: project::Command,
    },
    /// 領域登録簿（data/domains.csv）を操作する
    Domain {
        #[command(subcommand)]
        command: domain::Command,
    },
    /// 表形式データ（価格の履歴・計測値など）を SQLite に溜めて読む
    Data {
        #[command(subcommand)]
        command: data::Command,
    },
    /// kb/ 配下のファイルを読み書きする（どのディレクトリからでも実行できる）
    Kb {
        #[command(subcommand)]
        command: kb::Command,
    },
    /// KB をブラウザで読むローカルサーバーを立てる（読み取り専用・127.0.0.1 だけで待ち受ける）
    Serve(serve::Args),
    /// テンプレートからノートを作る
    New(new::Args),
    /// 外部のドキュメント（md・html・docx・xlsx・pptx・pdf・csv）を Markdown にして KB に取り込む
    Import(import::Args),
    /// フロントマター・登録簿・リンク・プラグインを検証する
    Check,
    /// 正本（docs/kb-format.md・templates/）をプラグインのスキルにコピーする
    Sync,
    /// 各クライアント向けに dist/ へ書き出す。html は KB のページを図と表の入った単独の HTML にする
    Export(export::Args),
    /// このリポジトリを手元のプラグイン置き場として Claude・Codex の設定に登録し、ika プラグインを入れる（公開・アップロードはしない）
    Install(install::Args),
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        // `ika ... | head` のようにパイプが先に閉じられたときは静かに終わる（`out.rs`）
        Err(err) if out::is_broken_pipe(&err) => ExitCode::from(out::BROKEN_PIPE_EXIT),
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> anyhow::Result<ExitCode> {
    let cli = Cli::parse();
    match cli.command {
        // KB のデータ（IKA_HOME）を扱うコマンド
        Command::Init(args) => init::run(&root::data_path()?, args)?,
        Command::Project { command } => project::run(&root::data_dir()?, command)?,
        Command::Domain { command } => domain::run(&root::data_dir()?, command)?,
        Command::Data { command } => data::run(&root::data_dir()?, command)?,
        Command::Kb { command } => kb::run(&root::data_dir()?, command)?,
        Command::Serve(args) => serve::run(&root::data_dir()?, args)?,
        Command::New(args) => new::run(&root::data_dir()?, args)?,
        Command::Import(args) => import::run(args)?,
        Command::Check => return check::run(&root::data_dir()?, root::repo().ok().as_deref()),
        // claude・chatgpt はリポジトリ、html は KB のデータを読む（中で分ける）
        Command::Export(args) => export::run(args)?,
        // ika のリポジトリ（プラグイン・ひな形）を扱う開発用コマンド
        Command::Sync => skills::sync(&root::repo()?)?,
        Command::Install(args) => install::run(&root::repo()?, args)?,
    }
    Ok(ExitCode::SUCCESS)
}
