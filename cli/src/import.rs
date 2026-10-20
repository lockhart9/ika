//! `ika import`: 外部のドキュメント（Markdown・HTML・Word・Excel・PowerPoint・PDF・CSV）を Markdown にして
//! KB に取り込む。全文を `type: source` として `kb/projects/<name>/sources/` か `kb/<領域>/sources/` に置き、
//! 原本（`source:`）とその内容のハッシュを持つ。同じ原本を取り込み直すと、変わっていたときだけ上書きする

mod convert;
mod docx;
mod pdf;
mod pptx;
mod sheet;
mod xml;

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::Local;
use sha2::{Digest, Sha256};

use crate::{check, frontmatter, kb, new, registry, root, vcs};
use convert::Format;

#[derive(clap::Args)]
pub struct Args {
    /// 取り込むファイル（複数可）。`-` で標準入力（MCP で取ってきた本文など。--source と --title が要る）
    #[arg(required_unless_present = "list")]
    paths: Vec<PathBuf>,
    /// プロダクトの資料として kb/projects/<name>/sources/ に置く。--domain と一緒なら関連プロダクトになる
    #[arg(short, long = "project")]
    projects: Vec<String>,
    /// 領域の資料として kb/<領域>/sources/ に置く（特定のプロダクトに属さない市場レポートなど）
    #[arg(short, long)]
    domain: Option<String>,
    /// タグ（カンマ区切り）
    #[arg(short, long, value_delimiter = ',')]
    tags: Vec<String>,
    /// 原本の URL やパス（取り込み直すときの目印）。省略時はファイルの絶対パス。ファイル1つのときだけ
    #[arg(long)]
    source: Option<String>,
    /// タイトル。省略時は文書のプロパティ・最初の見出し・ファイル名から。ファイル1つのときだけ
    #[arg(long)]
    title: Option<String>,
    /// ファイル名にする英字。省略時はタイトルかファイル名から。ファイル1つのときだけ
    #[arg(long)]
    slug: Option<String>,
    /// 形式。省略時は拡張子から（標準入力は md）
    #[arg(long, value_enum)]
    format: Option<Format>,
    /// KB に書かず、変換した Markdown を標準出力に出す
    #[arg(long, conflicts_with = "list")]
    print: bool,
    /// 取り込み済みの資料と原本の一覧（-p で絞れる）
    #[arg(long)]
    list: bool,
}

/// 読み込んだ原本1つ分
struct Input {
    bytes: Vec<u8>,
    format: Format,
    source: String,
    /// ファイル名（拡張子なし）。標準入力なら None
    stem: Option<String>,
    /// ファイルのあるディレクトリの名前（ファイル名が重なったときに前に付ける）。標準入力なら None
    parent: Option<String>,
}

pub fn run(args: Args) -> Result<()> {
    if args.list {
        return list(
            &root::data_dir()?,
            args.projects.first().map(String::as_str),
        );
    }
    if args.paths.len() > 1
        && (args.source.is_some() || args.title.is_some() || args.slug.is_some())
    {
        bail!("--source・--title・--slug はファイル1つのときだけ指定できます");
    }
    if args.print {
        for path in &args.paths {
            let input = read_input(path, &args)?;
            let converted = convert::convert(&input.bytes, input.format)?;
            warn(&converted.warnings);
            print!("{}", converted.body);
        }
        return Ok(());
    }

    let root = root::data_dir()?;
    let place = place(&root, &args)?;
    let mut existing = sources(&root)?
        .into_iter()
        .filter_map(|(rel, fields)| Some((fields.get("source")?.clone(), rel)))
        .collect::<HashMap<_, _>>();
    let mut changed = Vec::new();
    for path in &args.paths {
        let input = read_input(path, &args)?;
        let label = path.display().to_string();
        let rel = import_one(&root, &args, &place, &mut existing, input)
            .with_context(|| format!("{label} を取り込めません"))?;
        changed.extend(rel);
    }
    if !changed.is_empty() {
        let names: Vec<_> = changed.iter().map(|p| p.display().to_string()).collect();
        vcs::commit(&root, &format!("import {}", names.join(" ")));
    }
    Ok(())
}

/// 置き場所（kb/ からのディレクトリ）と、フロントマターの projects
struct Place {
    dir: PathBuf,
    projects: Vec<String>,
}

fn place(root: &Path, args: &Args) -> Result<Place> {
    let known = registry::projects(root)?;
    for project in &args.projects {
        if !known.iter().any(|p| &p.name == project) {
            bail!("未登録のプロダクトです: {project}（`ika project add` で登録してください）");
        }
    }
    let dir = match (&args.domain, args.projects.as_slice()) {
        (Some(domain), _) => {
            if !registry::domains(root)?.iter().any(|d| &d.name == domain) {
                bail!("未登録の領域です: {domain}（`ika domain list` で確認してください）");
            }
            Path::new("kb").join(domain).join("sources")
        }
        (None, [project]) => Path::new("kb/projects").join(project).join("sources"),
        (None, _) => bail!(
            "置き場所を --project <name>（プロダクトの資料）か --domain <領域>（領域の資料）で1つ指定してください"
        ),
    };
    Ok(Place {
        dir,
        projects: args.projects.clone(),
    })
}

fn read_input(path: &Path, args: &Args) -> Result<Input> {
    if path == Path::new("-") {
        let mut bytes = Vec::new();
        std::io::stdin().read_to_end(&mut bytes)?;
        let source = match (&args.source, args.print) {
            (Some(source), _) => source.clone(),
            (None, true) => "-".to_string(),
            (None, false) => {
                bail!("標準入力から取り込むときは --source で原本の URL を指定してください")
            }
        };
        return Ok(Input {
            bytes,
            format: args.format.unwrap_or(Format::Md),
            source,
            stem: None,
            parent: None,
        });
    }
    if path.is_dir() {
        bail!(
            "ディレクトリは渡せません。ファイルを指定してください（例: docs/*.md）: {}",
            path.display()
        );
    }
    let bytes = fs::read(path).with_context(|| format!("読めません: {}", path.display()))?;
    let format = match args.format {
        Some(format) => format,
        None => path
            .extension()
            .and_then(|e| Format::from_extension(&e.to_string_lossy()))
            .with_context(|| {
                format!(
                    "形式が分かりません: {}（md・txt・html・csv・docx・xlsx・xls・ods・pptx・pdf に対応。--format で指定できます）",
                    path.display()
                )
            })?,
    };
    let source = match &args.source {
        Some(source) => source.clone(),
        None => fs::canonicalize(path)?.display().to_string(),
    };
    Ok(Input {
        bytes,
        format,
        source,
        stem: path.file_stem().map(|s| s.to_string_lossy().into_owned()),
        parent: fs::canonicalize(path)?
            .parent()
            .and_then(|p| p.file_name())
            .map(|p| p.to_string_lossy().into_owned()),
    })
}

/// 1つ取り込む。書いたファイル（変更が無ければ None）を返す
fn import_one(
    root: &Path,
    args: &Args,
    place: &Place,
    existing: &mut HashMap<String, PathBuf>,
    input: Input,
) -> Result<Option<PathBuf>> {
    let converted = convert::convert(&input.bytes, input.format)?;
    warn(&converted.warnings);
    let hash = hex(&Sha256::digest(&input.bytes));
    let today = Local::now().date_naive().format("%Y-%m-%d").to_string();
    let mut doc = Doc {
        title: String::new(),
        date: today,
        projects: place.projects.join(", "),
        tags: args.tags.join(", "),
        source: input.source.clone(),
        format: input.format.name(),
        hash,
        body: converted.body,
    };

    if let Some(rel) = existing.get(&input.source) {
        let abs = root.join(rel);
        let current = fs::read_to_string(&abs)?;
        let fields = frontmatter::parse(&current)?.unwrap_or_default();
        let field = |key: &str| fields.get(key).cloned().unwrap_or_default();
        if field("hash") == doc.hash {
            println!("変更なし: {}", rel.display());
            return Ok(None);
        }
        // 取り込み直し。タイトル・タグ・プロダクトは指定が無ければ前のまま
        doc.title = args.title.clone().unwrap_or_else(|| field("title"));
        if args.tags.is_empty() {
            doc.tags = list_inner(&field("tags"));
        }
        doc.projects = list_inner(&field("projects"));
        let text = doc.render();
        kb::atomic::overwrite(&abs, &text)?;
        let (added, removed) = diff_stats(frontmatter::body(&current), frontmatter::body(&text));
        kb::done(
            root,
            &format!("取り込み直しました（+{added} -{removed} 行）"),
            rel,
            &text,
        )?;
        return Ok(Some(rel.clone()));
    }

    doc.title = args
        .title
        .clone()
        .or(converted.title)
        .or_else(|| input.stem.clone())
        .context("標準入力から取り込むときは --title でタイトルを指定してください")?
        .replace(['\n', '\r'], " ");
    let candidates = slug_candidates(args.slug.as_deref(), &doc.title, &input);
    if candidates.is_empty() {
        bail!("ファイル名にする英字を作れません。--slug で内容が分かる英字を指定してください");
    }
    let Some(rel) = candidates
        .iter()
        .map(|slug| place.dir.join(format!("{slug}.md")))
        .find(|rel| !root.join(rel).exists())
    else {
        let tried: Vec<_> = candidates.iter().map(|s| format!("{s}.md")).collect();
        bail!(
            "{} はすでに別の原本の取り込みに使われています。--slug で別の名前を指定してください",
            tried.join("・")
        );
    };
    let abs = root.join(&rel);
    let text = doc.render();
    fs::create_dir_all(abs.parent().expect("kb 配下のパス"))?;
    kb::atomic::create_new(&abs, &text)?;
    kb::done(root, "取り込みました", &rel, &text)?;
    existing.insert(input.source, rel.clone());
    Ok(Some(rel))
}

/// ファイル名（拡張子なし）の候補を、使う順に並べる。
/// - `--slug` があればそれだけ
/// - ファイルなら原本のファイル名。重なったときのために「ディレクトリ名-ファイル名」も足す
///   （`docs/<機能>/requirements.md` のように同じ名前が並ぶため）。ファイル名が英字にならなければタイトル
/// - 標準入力ならタイトル
fn slug_candidates(explicit: Option<&str>, title: &str, input: &Input) -> Vec<String> {
    let mut candidates = match (explicit, &input.stem) {
        (Some(slug), _) => vec![new::slugify(slug)],
        (None, Some(stem)) => {
            let mut list = vec![new::slugify(stem)];
            if let Some(parent) = &input.parent {
                list.push(new::slugify(&format!("{parent}-{stem}")));
            }
            list.push(new::slugify(title));
            list
        }
        (None, None) => vec![new::slugify(title)],
    };
    candidates.retain(|s| !s.is_empty());
    candidates.dedup();
    candidates
}

struct Doc {
    title: String,
    date: String,
    projects: String,
    tags: String,
    source: String,
    format: &'static str,
    hash: String,
    body: String,
}

impl Doc {
    fn render(&self) -> String {
        format!(
            "---\ntitle: {}\ntype: source\ndate: {}\nprojects: [{}]\ntags: [{}]\nsource: {}\nformat: {}\nhash: {}\n---\n\n\
             {NOTICE}{}（{}。{} に `ika import` で取り込み）。取り込み直すと上書きされるので、このファイルは直接編集しない。\
             要点や判断は README・要件書・ノートに書き、ここへリンクする。\n\n{}",
            self.title,
            self.date,
            self.projects,
            self.tags,
            self.source,
            self.format,
            self.hash,
            self.source,
            self.format,
            self.date,
            self.body
        )
    }
}

/// 取り込んだ資料の冒頭に付ける注意書きの書き出し
const NOTICE: &str = "> 原本: ";

/// `[a, b]` の中身（`a, b`）
fn list_inner(value: &str) -> String {
    frontmatter::list(value).unwrap_or_default().join(", ")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn warn(warnings: &[String]) {
    for warning in warnings {
        eprintln!("注意: {warning}");
    }
}

/// 増えた行と減った行の数（並びは見ない。冒頭の注意書きは数えない）
fn diff_stats(old: &str, new: &str) -> (usize, usize) {
    let mut counts: HashMap<&str, isize> = HashMap::new();
    for line in old.lines().filter(|l| !l.starts_with(NOTICE)) {
        *counts.entry(line).or_default() -= 1;
    }
    for line in new.lines().filter(|l| !l.starts_with(NOTICE)) {
        *counts.entry(line).or_default() += 1;
    }
    counts.values().fold((0, 0), |(added, removed), &n| {
        if n > 0 {
            (added + n as usize, removed)
        } else {
            (added, removed + n.unsigned_abs())
        }
    })
}

/// 取り込み済みの資料（type: source）。(kb からの相対パス, フロントマター)
fn sources(root: &Path) -> Result<Vec<(PathBuf, BTreeMap<String, String>)>> {
    let mut files = Vec::new();
    check::collect_markdown(&root.join("kb"), &mut files)?;
    files.sort();
    let mut found = Vec::new();
    for file in files {
        let rel = file.strip_prefix(root).unwrap_or(&file).to_path_buf();
        if !is_source_path(&rel) {
            continue;
        }
        let text = fs::read_to_string(&file)?;
        if let Ok(Some(fields)) = frontmatter::parse(&text)
            && fields.get("type").is_some_and(|t| t == "source")
        {
            found.push((rel, fields));
        }
    }
    Ok(found)
}

/// 取り込んだ資料の置き場所（kb/<領域>/sources/・kb/projects/<name>/sources/）にあるか
pub fn is_source_path(rel: &Path) -> bool {
    let parts: Vec<_> = rel.iter().map(|p| p.to_string_lossy()).collect();
    match parts.get(1).map(|p| p.as_ref()) {
        Some("projects") => parts.get(3).is_some_and(|p| p == "sources"),
        Some(_) => parts.get(2).is_some_and(|p| p == "sources"),
        None => false,
    }
}

fn list(root: &Path, project: Option<&str>) -> Result<()> {
    let mut count = 0;
    for (rel, fields) in sources(root)? {
        let field = |key: &str| fields.get(key).map(String::as_str).unwrap_or("");
        if let Some(project) = project {
            let projects = frontmatter::list(field("projects")).unwrap_or_default();
            if !projects.iter().any(|p| p == project) {
                continue;
            }
        }
        println!(
            "{}  [{}] {}  {}\n    原本: {}",
            rel.display(),
            field("format"),
            field("date"),
            field("title"),
            field("source")
        );
        count += 1;
    }
    eprintln!("{count} 件");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_stats_counts_changed_lines_but_not_the_notice() {
        let old = "> 原本: x（2026-10-01）\n\na\nb\nc\n";
        let new = "> 原本: x（2026-10-21）\n\na\nc\nd\ne\n";
        assert_eq!(diff_stats(old, new), (2, 1));
    }

    #[test]
    fn slugs_prefer_the_file_name_then_the_directory() {
        let input = |stem: Option<&str>, parent: Option<&str>| Input {
            bytes: Vec::new(),
            format: Format::Md,
            source: String::new(),
            stem: stem.map(String::from),
            parent: parent.map(String::from),
        };
        let file = input(Some("requirements"), Some("20260930-web-push"));
        assert_eq!(
            slug_candidates(None, "Web Push 通知 — 要件定義", &file),
            ["requirements", "20260930-web-push-requirements", "web-push"]
        );
        assert_eq!(
            slug_candidates(Some("Push Spec"), "x", &file),
            ["push-spec"]
        );
        assert_eq!(
            slug_candidates(None, "Pricing Report", &input(Some("料金"), Some("資料"))),
            ["pricing-report"]
        );
        assert_eq!(
            slug_candidates(None, "Notion Page", &input(None, None)),
            ["notion-page"]
        );
        assert!(slug_candidates(None, "料金", &input(None, None)).is_empty());
    }

    #[test]
    fn source_paths_are_recognized() {
        assert!(is_source_path(Path::new("kb/projects/foo/sources/spec.md")));
        assert!(is_source_path(Path::new("kb/business/sources/report.md")));
        assert!(!is_source_path(Path::new("kb/projects/foo/README.md")));
        assert!(!is_source_path(Path::new("kb/business/notes/2026/x.md")));
    }
}
