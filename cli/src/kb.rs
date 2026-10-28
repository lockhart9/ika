//! kb/ 配下のファイルを CLI 経由で読み書きする。
//! 外部プロジェクトの作業中に ika へ直接書くと権限確認が毎回出るので、
//! `ika kb ...` の1コマンドに寄せて許可ルールを1つで済ませる。

use std::fs;
use std::io::{IsTerminal, Read, Write};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Subcommand;

use crate::check;
use crate::registry;
use crate::root::expand_tilde;
use crate::vcs;

pub mod atomic;
pub mod cat;
pub mod fts;
pub mod grep;
pub mod orphans;
pub mod review;
pub mod search;
pub mod stale;
pub mod suggest;
pub mod triage;

#[derive(Subcommand)]
pub enum Command {
    /// Markdown を一覧する
    Ls {
        /// kb/ 配下のディレクトリ（省略時は kb/）
        path: Option<String>,
    },
    /// 本文とフロントマターで検索する
    Search(search::Args),
    /// 日本語の形態素解析つきの全文検索。関連度順に並べる（索引は IKA_HOME/.cache/ に作り、自動で更新する）
    Fts(fts::Args),
    /// 全文検索（fts）の索引に kb/ の変更を反映する（検索のたびにも自動で反映される）
    Index {
        /// 索引を消して一から作り直す
        #[arg(long)]
        rebuild: bool,
    },
    /// 引数をそのまま ripgrep に渡して kb/ を検索する（正規表現・文脈行など rg の機能を使う。要 rg）
    #[command(disable_help_flag = true)]
    Grep(grep::Args),
    /// どの insight・decision・プロダクトの資料（README を除く）からもリンクされていないノートを、プロダクトごとに数える
    /// （蒸留の候補。3 件以上たまったプロダクトに「要整理」を付ける）
    Orphans(orphans::Args),
    /// 古くなっていそうなナレッジを洗い出す（リポジトリより古い README、新しいノートを取り込んでいない知見、
    /// 古い価格・競合などを根拠にした資料、原本が変わった取り込み資料）
    Stale(stale::Args),
    /// 未整理のノートと古くなっていそうなナレッジを、優先順位つきの「今週やること」にまとめる
    /// （`orphans` と `stale` の判定を使う。読むだけで何も書かない）
    Triage(triage::Args),
    /// 確かめ直したナレッジに、確かめた日（フロントマターの reviewed）を付ける。内容を確かめてから実行する。
    /// stale・triage は date と reviewed の遅いほうを基準にする。1つでも付けられなければ何も書かない
    Review(review::Args),
    /// ファイルの中身を表示する。--outline・--section・--lines・--frontmatter で一部だけ読める
    Cat(cat::Args),
    /// 標準入力の内容でファイルを作る（フロントマターを含む全文）
    Write {
        /// kb/ から始まるパス（例: kb/tech/notes/2026/2026-10-18-foo.md）
        path: String,
        /// 既存ファイルを上書きする
        #[arg(long)]
        overwrite: bool,
    },
    /// 標準入力の内容をファイルの末尾に足す
    Append { path: String },
    /// 文字列を置き換える。--old はファイル内で1か所だけに一致する必要がある
    Edit {
        path: String,
        /// 置き換える文字列（`- 項目` のように `-` で始まってもよい）
        #[arg(long, allow_hyphen_values = true)]
        old: String,
        /// 置き換え後の文字列（`-` で始まってもよい）
        #[arg(long, allow_hyphen_values = true)]
        new: String,
        /// 複数あっても全部置き換える
        #[arg(long)]
        all: bool,
    },
    /// ファイルを削除する（ディレクトリと、登録済みプロダクトの README は削除できない）。
    /// 他のノートからリンクされていれば、リンク切れになる参照元を表示する
    Rm { path: String },
    /// ファイルを移動・改名する。移動先が存在するときは失敗し、
    /// 移動で切れたリンク（参照元と、移動したファイル自身のリンク）を表示する
    Mv {
        from: String,
        to: String,
        /// 切れるリンクを自動で書き換える（参照元の `](旧パス)` と、移動したファイル自身の相対リンク）
        #[arg(long)]
        fix_links: bool,
    },
}

pub fn run(root: &Path, command: Command) -> Result<()> {
    match command {
        Command::Ls { path } => {
            let rel = match path {
                Some(path) => resolve(root, &path)?,
                None => PathBuf::from("kb"),
            };
            guard_symlink(root, &rel)?;
            let mut files = Vec::new();
            check::collect_markdown(&root.join(&rel), &mut files)?;
            files.sort();
            for file in files {
                println!("{}", file.strip_prefix(root).unwrap_or(&file).display());
            }
        }
        Command::Search(args) => search::run(root, args)?,
        Command::Fts(args) => fts::run(root, args)?,
        Command::Index { rebuild } => fts::index(root, rebuild)?,
        Command::Grep(args) => grep::run(root, args)?,
        Command::Orphans(args) => orphans::run(root, args)?,
        Command::Stale(args) => stale::run(root, args)?,
        Command::Triage(args) => triage::run(root, args)?,
        Command::Review(args) => review::run(root, args)?,
        Command::Cat(args) => cat::run(root, args)?,
        Command::Write { path, overwrite } => {
            let rel = resolve(root, &path)?;
            guard_symlink(root, &rel)?;
            let abs = root.join(&rel);
            if abs.exists() && !overwrite {
                bail!(
                    "すでに存在します: {}（上書きするなら --overwrite）",
                    rel.display()
                );
            }
            let text = with_newline(read_stdin()?);
            fs::create_dir_all(abs.parent().expect("kb 配下のパス"))?;
            if overwrite {
                atomic::overwrite(&abs, &text)?;
            } else {
                atomic::create_new(&abs, &text)?;
            }
            done(root, "書きました", &rel, &text)?;
            if !overwrite {
                suggest::after_write(root, &rel, &text);
            }
            vcs::commit(root, &format!("kb write {}", rel.display()));
        }
        Command::Append { path } => {
            let rel = resolve(root, &path)?;
            guard_symlink(root, &rel)?;
            let addition = with_newline(read_stdin()?);
            let text = atomic::modify(&root.join(&rel), |current| {
                let mut text = with_newline(current.to_string());
                text.push_str(&addition);
                Ok(text)
            })?;
            done(root, "追記しました", &rel, &text)?;
            vcs::commit(root, &format!("kb append {}", rel.display()));
        }
        Command::Edit {
            path,
            old,
            new,
            all,
        } => {
            let rel = resolve(root, &path)?;
            guard_symlink(root, &rel)?;
            let text = atomic::modify(&root.join(&rel), |current| {
                replace(current, &old, &new, all)
            })?;
            done(root, "編集しました", &rel, &text)?;
            vcs::commit(root, &format!("kb edit {}", rel.display()));
        }
        Command::Rm { path } => {
            let rel = resolve(root, &path)?;
            guard_symlink(root, &rel)?;
            let abs = root.join(&rel);
            if !abs.is_file() {
                bail!("ファイルがありません: {}", rel.display());
            }
            refuse_registered_readme(root, &abs, "削除")?;
            let referrers = inbound(root, &rel)?;
            fs::remove_file(&abs)?;
            println!("削除しました: {}", rel.display());
            warn_broken("リンク切れになります", &referrers);
            vcs::commit(root, &format!("kb rm {}", rel.display()));
        }
        Command::Mv {
            from,
            to,
            fix_links,
        } => {
            let from = resolve(root, &from)?;
            let to = resolve(root, &to)?;
            guard_symlink(root, &from)?;
            guard_symlink(root, &to)?;
            let (src, dst) = (root.join(&from), root.join(&to));
            if !src.is_file() {
                bail!("ファイルがありません: {}", from.display());
            }
            if dst.exists() {
                bail!("移動先がすでに存在します: {}", to.display());
            }
            refuse_registered_readme(root, &src, "移動")?;
            fs::create_dir_all(dst.parent().expect("kb 配下のパス"))?;
            fs::rename(&src, &dst)?;
            println!("移動しました: {} -> {}", from.display(), to.display());
            if fix_links {
                fix_links_after_move(root, &from, &to)?;
            } else {
                let referrers = inbound(root, &from)?;
                warn_broken("リンク切れになりました", &referrers);
                if !referrers.is_empty() {
                    println!("（`--fix-links` を付けると自動で書き換えます）");
                }
            }
            vcs::commit(root, &format!("kb mv {} {}", from.display(), to.display()));
            if to.extension().is_some_and(|ext| ext == "md") {
                report(root, &to, &read(root, &to)?)?;
            }
        }
    }
    Ok(())
}

/// 引数のパスを「ルートからの相対パス」にそろえる。kb/ の外は受け付けない
pub fn resolve(root: &Path, input: &str) -> Result<PathBuf> {
    let path = expand_tilde(input);
    let path = if path.is_absolute() {
        path.strip_prefix(root)
            .with_context(|| format!("ika のリポジトリの外です: {input}"))?
            .to_path_buf()
    } else {
        path
    };
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            _ => bail!("`..` は使えません: {input}"),
        }
    }
    if !clean.starts_with("kb") {
        bail!("kb/ 配下のパスだけ操作できます: {input}");
    }
    Ok(clean)
}

/// kb/ の下にシンボリックリンクがあれば拒否する（リンク切れも含む。外へ書き出せてしまうため）。
/// `kb` 自体は基点なので対象外
pub fn guard_symlink(root: &Path, rel: &Path) -> Result<()> {
    let mut path = root.join("kb");
    for component in rel.components().skip(1) {
        path.push(component);
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                bail!("シンボリックリンクは辿れません: {}", rel.display())
            }
            Ok(_) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => break,
            Err(err) => return Err(err.into()),
        }
    }
    Ok(())
}

fn read(root: &Path, rel: &Path) -> Result<String> {
    fs::read_to_string(root.join(rel)).with_context(|| format!("読めません: {}", rel.display()))
}

/// 標準入力を最後まで読む。端末につながっていたら待ち続けないようにエラーにする
pub fn read_stdin() -> Result<String> {
    if std::io::stdin().is_terminal() {
        bail!("標準入力から内容を渡してください（例: ika kb write <path> <<'EOF' ... EOF）");
    }
    let mut text = String::new();
    std::io::stdin().read_to_string(&mut text)?;
    Ok(text)
}

fn with_newline(mut text: String) -> String {
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

/// `old` を `new` に置き換える。`all` でなければ一致がちょうど1か所のときだけ
fn replace(text: &str, old: &str, new: &str, all: bool) -> Result<String> {
    if old.is_empty() {
        bail!("--old が空です");
    }
    match text.matches(old).count() {
        0 => bail!("--old に一致する箇所がありません"),
        1 => Ok(text.replacen(old, new, 1)),
        _ if all => Ok(text.replace(old, new)),
        n => {
            bail!("--old が {n} か所に一致します。前後を足して1か所に絞るか --all を付けてください")
        }
    }
}

/// 登録済みプロダクトの README は、登録簿と食い違うので触らせない
fn refuse_registered_readme(root: &Path, abs: &Path, action: &str) -> Result<()> {
    if let Some(project) = registry::projects(root)?
        .iter()
        .find(|p| registry::readme_path(root, &p.name) == abs)
    {
        bail!(
            "登録済みプロダクト {} の README は{action}できません（data/projects.csv を直すまで残します）",
            project.name
        );
    }
    Ok(())
}

/// 書いた結果を報告し、Markdown なら検証の指摘を添える
pub fn done(root: &Path, verb: &str, rel: &Path, text: &str) -> Result<()> {
    println!("{verb}: {}", rel.display());
    report(root, rel, text)
}

/// Markdown の検証の指摘を表示する
fn report(root: &Path, rel: &Path, text: &str) -> Result<()> {
    if rel.extension().is_some_and(|ext| ext == "md") {
        let problems = check::file(root, rel, text)?;
        if !problems.is_empty() {
            let mut out = std::io::stdout().lock();
            writeln!(out, "要修正（`ika kb edit` で直してください）:")?;
            for problem in problems {
                writeln!(out, "  - {problem}")?;
            }
        }
    }
    Ok(())
}

/// `target`（ルートからの相対パス）へリンクしている kb/ 内のファイル。`target` 自身は除く
pub fn inbound(root: &Path, target: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    check::collect_markdown(&root.join("kb"), &mut files)?;
    files.sort();
    let mut referrers = Vec::new();
    for file in files {
        let rel = file.strip_prefix(root).unwrap_or(&file);
        if rel == target {
            continue;
        }
        let dir = rel.parent().unwrap_or(Path::new(""));
        let links = check::link_targets(&fs::read_to_string(&file)?);
        if links
            .iter()
            .any(|link| normalize(&dir.join(link)) == target)
        {
            referrers.push(rel.to_path_buf());
        }
    }
    Ok(referrers)
}

/// `.` と `..` を字面で畳む（ファイルが無くても使える）
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// 移動したファイルへのリンク（参照元）と、移動したファイル自身の相対リンクを書き換える
fn fix_links_after_move(root: &Path, from: &Path, to: &Path) -> Result<()> {
    let mut fixed = Vec::new();
    for referrer in inbound(root, from)? {
        let dir = referrer.parent().unwrap_or(Path::new(""));
        atomic::modify(&root.join(&referrer), |text| {
            Ok(check::map_links(text, |link| {
                (normalize(&dir.join(link)) == from).then(|| relative_path(dir, to))
            }))
        })?;
        fixed.push(referrer);
    }

    // 自身のリンクは、移動前の置き場所を基準に解決し直す。移動前から切れていたものは触らない
    if to.extension().is_some_and(|ext| ext == "md") {
        let old_dir = from.parent().unwrap_or(Path::new(""));
        let new_dir = to.parent().unwrap_or(Path::new(""));
        let before = fs::read_to_string(root.join(to))?;
        let after = atomic::modify(&root.join(to), |text| {
            Ok(check::map_links(text, |link| {
                let target = normalize(&old_dir.join(link));
                let target = if target == from {
                    to.to_path_buf()
                } else if root.join(&target).exists() {
                    target
                } else {
                    return None;
                };
                Some(relative_path(new_dir, &target)).filter(|new| new != link)
            }))
        })?;
        if after != before {
            fixed.push(to.to_path_buf());
        }
    }

    if fixed.is_empty() {
        println!("書き換えるリンクはありませんでした");
    } else {
        println!("リンクを書き換えました:");
        for file in fixed {
            println!("  - {}", file.display());
        }
    }
    Ok(())
}

/// `dir` からの `target` への相対パス（`/` 区切り）。どちらもルートからの相対パス
fn relative_path(dir: &Path, target: &Path) -> String {
    let dir: Vec<_> = dir.components().collect();
    let target: Vec<_> = target.components().collect();
    let common = dir.iter().zip(&target).take_while(|(a, b)| a == b).count();
    let mut parts = vec!["..".to_string(); dir.len() - common];
    parts.extend(
        target[common..]
            .iter()
            .map(|c| c.as_os_str().to_string_lossy().into_owned()),
    );
    parts.join("/")
}

fn warn_broken(what: &str, referrers: &[PathBuf]) {
    if referrers.is_empty() {
        return;
    }
    println!("参照元のリンクが{what}（`ika kb edit` で直してください）:");
    for referrer in referrers {
        println!("  - {}", referrer.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT: &str = "/repo/ika";

    fn rel(input: &str) -> Result<PathBuf> {
        resolve(Path::new(ROOT), input)
    }

    #[test]
    fn resolve_accepts_paths_under_kb() {
        let want = PathBuf::from("kb/tech/notes/a.md");
        assert_eq!(rel("kb/tech/notes/a.md").unwrap(), want);
        assert_eq!(rel("./kb/tech/notes/a.md").unwrap(), want);
        assert_eq!(rel("/repo/ika/kb/tech/notes/a.md").unwrap(), want);
    }

    #[test]
    fn resolve_rejects_paths_outside_kb() {
        assert!(rel("kb/../data/projects.csv").is_err());
        assert!(rel("data/projects.csv").is_err());
        assert!(rel("kbx/a.md").is_err());
        assert!(rel("/etc/passwd").is_err());
        assert!(rel("/repo/other/kb/a.md").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn guard_rejects_symlinks_including_dangling() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!("ika-guard-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("kb/tech")).unwrap();
        symlink(root.join("outside/new.md"), root.join("kb/dangling.md")).unwrap();
        symlink(&root, root.join("kb/dir")).unwrap();
        fs::write(root.join("kb/tech/a.md"), "").unwrap();

        assert!(guard_symlink(&root, Path::new("kb/dangling.md")).is_err());
        assert!(guard_symlink(&root, Path::new("kb/dir/x.md")).is_err());
        assert!(guard_symlink(&root, Path::new("kb/tech/a.md")).is_ok());
        assert!(guard_symlink(&root, Path::new("kb/tech/new/b.md")).is_ok());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn relative_path_goes_up_and_down() {
        let rel = |dir: &str, target: &str| relative_path(Path::new(dir), Path::new(target));
        assert_eq!(
            rel("kb/tech/insights", "kb/tech/notes/2026/a.md"),
            "../notes/2026/a.md"
        );
        assert_eq!(rel("kb/tech/notes", "kb/tech/notes/a.md"), "a.md");
        assert_eq!(rel("kb/tech/notes/2026", "kb/tech/a.md"), "../../a.md");
    }

    #[test]
    fn normalize_folds_dot_segments() {
        assert_eq!(
            normalize(Path::new("kb/tech/notes/2026/../../insights/./a.md")),
            PathBuf::from("kb/tech/insights/a.md")
        );
    }

    #[test]
    fn replace_requires_unique_match() {
        assert_eq!(replace("a b a", "b", "c", false).unwrap(), "a c a");
        assert!(replace("a b a", "a", "c", false).is_err());
        assert_eq!(replace("a b a", "a", "c", true).unwrap(), "c b c");
        assert!(replace("a", "x", "y", true).is_err());
        assert!(replace("a", "", "y", false).is_err());
    }
}
