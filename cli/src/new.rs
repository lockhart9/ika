use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use chrono::{Local, NaiveDate};
use clap::ValueEnum;

use crate::frontmatter;
use crate::kb;
use crate::registry;
use crate::templates;
use crate::vcs;

#[derive(clap::Args)]
pub struct Args {
    /// ノートの種類
    kind: Kind,
    /// タイトル
    title: String,
    /// 領域（data/domains.csv の name。例: business, tech）
    #[arg(short, long)]
    domain: String,
    /// 関連プロダクト（複数指定可）
    #[arg(short, long = "project")]
    projects: Vec<String>,
    /// タグ（カンマ区切り）
    #[arg(short, long, value_delimiter = ',')]
    tags: Vec<String>,
    /// ファイル名に使う英字のスラッグ。省略時はタイトルから作る。日本語だけのタイトルでは必須
    #[arg(short, long)]
    slug: Option<String>,
    /// 日付（YYYY-MM-DD）。過去の出来事や決定を記録するときに使う。省略時は今日
    #[arg(long)]
    date: Option<NaiveDate>,
    /// 標準入力を本文にする（テンプレートの本文の代わり。フロントマターはそのまま）
    #[arg(long)]
    stdin: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum Kind {
    Note,
    Insight,
    Decision,
}

impl Kind {
    fn template(self) -> &'static str {
        match self {
            Kind::Note => "note",
            Kind::Insight => "insight",
            Kind::Decision => "decision",
        }
    }
}

pub fn run(root: &Path, args: Args) -> Result<()> {
    if !registry::domains(root)?
        .iter()
        .any(|d| d.name == args.domain)
    {
        bail!(
            "未登録の領域です: {}（`ika domain list` で確認してください）",
            args.domain
        );
    }
    let known = registry::projects(root)?;
    for project in &args.projects {
        if !known.iter().any(|p| &p.name == project) {
            bail!("未登録のプロダクトです: {project}（`ika project add` で登録してください）");
        }
    }

    let now = Local::now();
    let date = args
        .date
        .unwrap_or_else(|| now.date_naive())
        .format("%Y-%m-%d")
        .to_string();
    let slug = match &args.slug {
        Some(slug) => slugify(slug),
        None => slugify(&args.title),
    };
    if slug.is_empty() {
        bail!(
            "ファイル名にする英字のスラッグを作れません。--slug で内容が分かる英字を指定してください（例: --slug pricing-review）"
        );
    }

    let path = target_path(root, &args.domain, args.kind, &date, &slug);
    if path.exists() {
        bail!("すでに存在します: {}", path.display());
    }

    let template = templates::get(args.kind.template()).expect("埋め込み済みのひな形");
    let content = render(
        template,
        &[
            ("title", &args.title),
            ("date", &date),
            ("projects", &args.projects.join(", ")),
            ("tags", &args.tags.join(", ")),
        ],
    );
    let content = if args.stdin {
        format!(
            "{}\n{}",
            frontmatter::head(&content),
            crate::kb::read_stdin()?
        )
    } else {
        content
    };
    fs::create_dir_all(path.parent().expect("kb 配下のパス"))?;
    kb::atomic::create_new(&path, &content)?;
    let rel = path.strip_prefix(root).unwrap_or(&path);
    println!("{}", rel.display());
    vcs::commit(
        root,
        &format!("new {}: {}", args.kind.template(), rel.display()),
    );
    Ok(())
}

fn target_path(root: &Path, domain: &str, kind: Kind, date: &str, slug: &str) -> PathBuf {
    let kb = root.join("kb").join(domain);
    match kind {
        Kind::Note => kb
            .join("notes")
            .join(&date[..4])
            .join(format!("{date}-{slug}.md")),
        Kind::Insight => kb.join("insights").join(format!("{slug}.md")),
        Kind::Decision => kb.join("decisions").join(format!("{date}-{slug}.md")),
    }
}

/// `{{key}}` を置き換える
pub fn render(template: &str, vars: &[(&str, &str)]) -> String {
    vars.iter().fold(template.to_string(), |acc, (key, value)| {
        acc.replace(&format!("{{{{{key}}}}}"), value)
    })
}

/// 英数字以外をハイフンにまとめ、小文字にする。日本語だけのタイトルは空になる
fn slugify(text: &str) -> String {
    let mut slug = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_end_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_keeps_ascii_words() {
        assert_eq!(slugify("Alpha の Beta 設計"), "alpha-beta");
        assert_eq!(slugify("料金の見直し"), "");
    }

    #[test]
    fn render_replaces_placeholders() {
        assert_eq!(
            render("{{a}}-{{b}}-{{a}}", &[("a", "1"), ("b", "2")]),
            "1-2-1"
        );
    }
}
