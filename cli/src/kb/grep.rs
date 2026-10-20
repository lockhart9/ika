//! `ika kb grep`: ripgrep で kb/ を検索する（正規表現・文脈行・`--json` など rg の機能を使う）
//!
//! kb/ の外を読ませないため、rg のオプションは許可したものだけ通し、パスは全部 kb/ の中か確かめてから渡す

use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

#[derive(clap::Args)]
pub struct Args {
    /// ripgrep に渡す引数（例: `-n -C2 'review|レビュー' kb/tech`）。パスは IKA_HOME からの相対（kb/ から始める）。
    /// 検索は kb/ の中だけに限る
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub(crate) args: Vec<String>,
}

/// 値を取らない短いオプション
const SHORT_FLAGS: &str = "abcFHiIlnNopPqsSUvwx0";
/// 値を取る短いオプション（`-C2` のようにくっつけても、`-C 2` のように分けてもよい）
const SHORT_VALUES: &str = "ABCeEgjmMrtT";

/// 値を取らない長いオプション
const LONG_FLAGS: &[&str] = &[
    "byte-offset",
    "case-sensitive",
    "column",
    "count",
    "count-matches",
    "crlf",
    "files",
    "files-with-matches",
    "files-without-match",
    "fixed-strings",
    "glob-case-insensitive",
    "heading",
    "hidden",
    "ignore-case",
    "include-zero",
    "invert-match",
    "json",
    "line-number",
    "line-regexp",
    "max-columns-preview",
    "multiline",
    "multiline-dotall",
    "no-column",
    "no-filename",
    "no-heading",
    "no-ignore",
    "no-line-number",
    "no-messages",
    "no-unicode",
    "null",
    "null-data",
    "only-matching",
    "passthru",
    "pcre2",
    "pretty",
    "quiet",
    "smart-case",
    "stats",
    "text",
    "trim",
    "vimgrep",
    "with-filename",
    "word-regexp",
];

/// 値を取る長いオプション（`--context=2` でも `--context 2` でもよい）
const LONG_VALUES: &[&str] = &[
    "after-context",
    "before-context",
    "color",
    "colors",
    "context",
    "context-separator",
    "encoding",
    "engine",
    "field-context-separator",
    "field-match-separator",
    "glob",
    "iglob",
    "max-columns",
    "max-count",
    "max-depth",
    "max-filesize",
    "regexp",
    "replace",
    "sort",
    "sortr",
    "threads",
    "type",
    "type-not",
];

pub fn run(root: &Path, args: Args) -> Result<()> {
    let argv = rg_args(root, &args.args)?;
    let status = Command::new("rg")
        .args(&argv)
        .current_dir(root)
        // 標準入力は検索しない（パス `-` を渡されても rg が stdin を読んで固まらないように）
        .stdin(Stdio::null())
        .status()
        .context("rg（ripgrep）を実行できません。インストールして PATH に通してください")?;
    // 0: 一致あり、1: 一致なし、2: エラー。rg の終了コードをそのまま返す
    match status.code() {
        Some(0) => Ok(()),
        Some(code) => std::process::exit(code),
        None => bail!("rg がシグナルで終了しました"),
    }
}

/// ユーザーの引数をオプションと位置引数（検索語・パス）に分けたもの
#[derive(Debug, Default, PartialEq)]
struct Parsed {
    options: Vec<String>,
    positionals: Vec<String>,
    /// `-e` / `--regexp` で検索語が渡された（位置引数は全部パス）
    has_regexp: bool,
    /// `--files`（検索語を取らず、位置引数は全部パス）
    files_mode: bool,
}

/// 許可したオプションだけを受け付けて引数を分ける
fn parse(user: &[String]) -> Result<Parsed> {
    let refuse = |opt: &str| -> anyhow::Error {
        anyhow::anyhow!(
            "{opt} は ika kb grep では使えません（kb/ の外を読む・外部コマンドを動かすおそれのないオプションだけ通します）"
        )
    };
    let mut parsed = Parsed::default();
    let mut it = user.iter();
    while let Some(arg) = it.next() {
        if arg == "--" {
            parsed.positionals.extend(it.by_ref().cloned());
            break;
        }
        let needs_value = if let Some(long) = arg.strip_prefix("--") {
            let (name, inline) = match long.split_once('=') {
                Some((name, value)) => (name, Some(value)),
                None => (long, None),
            };
            match name {
                "regexp" => parsed.has_regexp = true,
                "files" => parsed.files_mode = true,
                _ => {}
            }
            if LONG_FLAGS.contains(&name) {
                if inline.is_some() {
                    bail!("--{name} は値を取りません");
                }
                false
            } else if LONG_VALUES.contains(&name) {
                inline.is_none()
            } else {
                return Err(refuse(&format!("--{name}")));
            }
        } else if let Some(bundle) = arg.strip_prefix('-').filter(|b| !b.is_empty()) {
            // `-nC2` のようにまとめた短いオプションを1文字ずつ見る。値を取る文字があれば、残りがその値
            let mut needs_value = false;
            for (i, c) in bundle.char_indices() {
                if SHORT_FLAGS.contains(c) {
                    continue;
                }
                if !SHORT_VALUES.contains(c) {
                    return Err(refuse(&format!("-{c}")));
                }
                if c == 'e' {
                    parsed.has_regexp = true;
                }
                needs_value = bundle[i + c.len_utf8()..].is_empty();
                break;
            }
            needs_value
        } else {
            parsed.positionals.push(arg.clone());
            continue;
        };
        parsed.options.push(arg.clone());
        if needs_value {
            let Some(value) = it.next() else {
                bail!("{arg} に値がありません");
            };
            parsed.options.push(value.clone());
        }
    }
    Ok(parsed)
}

/// rg に渡す引数を作る: `--no-config` ユーザーのオプション `--no-follow` `--` 検索語 パス…
/// パスが無ければ kb/ 全体を検索する
fn rg_args(root: &Path, user: &[String]) -> Result<Vec<String>> {
    let Parsed {
        options,
        mut positionals,
        has_regexp,
        files_mode,
    } = parse(user)?;
    let pattern = if has_regexp || files_mode {
        None
    } else if positionals.is_empty() {
        bail!("検索語がありません（例: ika kb grep -n 'review|レビュー' kb/tech）");
    } else {
        Some(positionals.remove(0))
    };
    check_paths(root, &positionals)?;

    let mut argv = vec!["--no-config".to_string()];
    argv.extend(options);
    argv.push("--no-follow".to_string());
    argv.push("--".to_string());
    argv.extend(pattern);
    if positionals.is_empty() {
        argv.push("kb".to_string());
    } else {
        argv.extend(positionals);
    }
    Ok(argv)
}

/// パスはすべて kb/ の中に実在しなければならない（シンボリックリンクは辿った先で判定する）
fn check_paths(root: &Path, paths: &[String]) -> Result<()> {
    let kb = root.join("kb").canonicalize()?;
    for p in paths {
        let Ok(path) = root.join(p).canonicalize() else {
            bail!("見つかりません: {p}（パスは IKA_HOME からの相対で、kb/ から始めます）");
        };
        if !path.starts_with(&kb) {
            bail!("kb/ の外は検索できません: {p}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    /// kb/tech/a.md と、kb/ の外の data/x.csv を持つ IKA_HOME
    fn home(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("ika-grep-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("kb/tech")).unwrap();
        std::fs::create_dir_all(root.join("data")).unwrap();
        std::fs::write(root.join("kb/tech/a.md"), "x").unwrap();
        std::fs::write(root.join("data/x.csv"), "x").unwrap();
        root
    }

    #[test]
    fn searches_kb_when_no_path_is_given() {
        let root = home("default");
        assert_eq!(
            rg_args(&root, &v(&["-n", "-C2", "foo"])).unwrap(),
            v(&["--no-config", "-n", "-C2", "--no-follow", "--", "foo", "kb"])
        );
    }

    #[test]
    fn passes_paths_inside_kb() {
        let root = home("inside");
        assert_eq!(
            rg_args(&root, &v(&["-C", "2", "foo", "kb/tech"])).unwrap(),
            v(&[
                "--no-config",
                "-C",
                "2",
                "--no-follow",
                "--",
                "foo",
                "kb/tech"
            ])
        );
        // -e の後の位置引数は全部パス。`--` の後ろは `-` で始まってもオプション扱いしない
        assert_eq!(
            rg_args(&root, &v(&["-e", "-a", "--", "kb/tech/a.md"])).unwrap(),
            v(&[
                "--no-config",
                "-e",
                "-a",
                "--no-follow",
                "--",
                "kb/tech/a.md"
            ])
        );
    }

    #[test]
    fn refuses_paths_outside_kb() {
        let root = home("outside");
        for path in [
            ".",
            "data",
            "data/x.csv",
            "kb/../data",
            "/etc/hosts",
            "-",
            "nope",
        ] {
            assert!(rg_args(&root, &v(&["foo", path])).is_err(), "{path}");
        }
        assert!(rg_args(&root, &v(&["--files", "data"])).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlinks_leading_outside_kb() {
        let root = home("symlink");
        std::os::unix::fs::symlink(root.join("data"), root.join("kb/link")).unwrap();
        assert!(rg_args(&root, &v(&["foo", "kb/link"])).is_err());
    }

    #[test]
    fn refuses_options_outside_the_allowlist() {
        let root = home("options");
        for args in [
            &["--pre", "sh", "x"][..],
            &["--pre=sh", "x"],
            &["--pre-glob=*.md", "x"],
            &["--hostname-bin=sh", "x"],
            &["-L", "x"],
            &["-nL", "x"],
            &["--follow", "x"],
            &["-z", "x"],
            &["-f", "/etc/passwd"],
            &["--file=/etc/passwd"],
            &["--ignore-file", "x", "y"],
            &["--type-add", "md:*.md", "x"],
            &["--json=1", "x"],
        ] {
            assert!(rg_args(&root, &v(args)).is_err(), "{args:?}");
        }
    }

    #[test]
    fn splits_bundled_short_options() {
        let parsed = parse(&v(&["-niC", "3", "-wA2", "-e", "pat", "kb"])).unwrap();
        assert_eq!(parsed.options, v(&["-niC", "3", "-wA2", "-e", "pat"]));
        assert_eq!(parsed.positionals, v(&["kb"]));
        assert!(parsed.has_regexp);
        assert!(parse(&v(&["-C"])).is_err());
    }

    #[test]
    fn requires_a_pattern() {
        let root = home("pattern");
        assert!(rg_args(&root, &v(&["-n"])).is_err());
        assert_eq!(
            rg_args(&root, &v(&["--files"])).unwrap(),
            v(&["--no-config", "--files", "--no-follow", "--", "kb"])
        );
    }
}
