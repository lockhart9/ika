use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use crate::{check, frontmatter, registry, root as roots, skills};

#[derive(clap::Args)]
pub struct Args {
    /// 書き出し先のクライアント
    target: Target,
    /// （claude のみ）KB のスナップショットも dist/claude/ に書き出す。claude.ai のプロジェクトの
    /// 知識として上げる用で、読み取り専用。KB の内容がクラウドに上がるので、渡してよい範囲を確かめる
    #[arg(long)]
    with_kb: bool,
    /// 含める領域（複数指定可。省略時は全領域）。KB を書き出すときだけ効く
    #[arg(long = "domain")]
    domains: Vec<String>,
    /// プロダクト概要（kb/projects/）と projects.csv を含めない。KB を書き出すときだけ効く
    #[arg(long)]
    no_projects: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum Target {
    /// プラグインの zip（claude.ai・Claude Desktop の「Upload plugin」用）
    Claude,
    /// プロジェクトの指示文とナレッジ束（ChatGPT の Projects 用）
    Chatgpt,
}

/// 書き出す KB の範囲
struct Scope {
    domains: Vec<String>,
    projects: bool,
}

pub fn run(root: &Path, args: Args) -> Result<()> {
    let scope = Scope {
        domains: args.domains,
        projects: !args.no_projects,
    };
    match args.target {
        Target::Claude => {
            if !args.with_kb && (!scope.domains.is_empty() || !scope.projects) {
                bail!("--domain・--no-projects は --with-kb と一緒に使います");
            }
            claude(root, args.with_kb.then_some(&scope))
        }
        Target::Chatgpt => chatgpt(root, &scope),
    }
}

/// zip の直下にプラグインのフォルダ（ika/）を1つ置く。`kb` があれば KB のスナップショットも添える
fn claude(root: &Path, kb: Option<&Scope>) -> Result<()> {
    let out = root.join("dist/claude");
    fs::create_dir_all(&out)?;
    let path = out.join("ika.zip");
    let mut zip = ZipWriter::new(File::create(&path)?);
    add_dir(&mut zip, &root.join(skills::PLUGIN_DIR), "ika")?;
    zip.finish()?;
    println!("{}", path.strip_prefix(root).unwrap_or(&path).display());

    // 前回書き出した KB を残さない（範囲を絞り直したときに、古い内容を上げてしまわないように）
    remove_knowledge(&out)?;
    let _ = fs::remove_file(out.join("instructions.md"));
    let Some(scope) = kb else {
        return Ok(());
    };
    let home = roots::data_dir()?;
    let mut instructions = fs::read_to_string(root.join("AGENTS.md"))?;
    instructions.push_str(
        "\n## claude.ai での使い方\n\n\
         - ここでは `ika` CLI とローカルのファイルは使えない。ナレッジはプロジェクトの知識ファイルの `knowledge-<領域>.md`（領域ごと）、\
         `knowledge-projects.md`（プロダクト概要）、`projects.csv`・`domains.csv` にある。各ノートの先頭の `<!-- path: ... -->` が元のパス\n\
         - 知識ファイルは書き出した時点のスナップショットで、読み取り専用。記録を頼まれたら、保存先のパスとフロントマター付きの本文を出力する\n",
    );
    fs::write(out.join("instructions.md"), instructions)?;
    println!("dist/claude/instructions.md");
    for name in write_knowledge(&home, &out, scope)? {
        println!("dist/claude/{name}");
    }
    println!(
        "\n注意: KB の内容（上の knowledge-*.md と CSV）を含みます。上げる先（claude.ai のプロジェクト）に渡してよい範囲か確かめてください"
    );
    Ok(())
}

fn add_dir(zip: &mut ZipWriter<File>, dir: &Path, prefix: &str) -> Result<()> {
    let mut entries: Vec<_> = fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    for path in entries {
        let name = format!(
            "{prefix}/{}",
            path.file_name().expect("entry").to_string_lossy()
        );
        if path.is_dir() {
            add_dir(zip, &path, &name)?;
        } else {
            zip.start_file(name, SimpleFileOptions::default())?;
            zip.write_all(&fs::read(&path)?)?;
        }
    }
    Ok(())
}

/// ChatGPT はローカルファイルを読めないので、指示文とナレッジを数ファイルにまとめる
fn chatgpt(root: &Path, scope: &Scope) -> Result<()> {
    let out = root.join("dist/chatgpt");
    fs::create_dir_all(&out)?;
    // ナレッジ（kb/・登録簿）は IKA_HOME のデータ、指示文・スキル・書式はリポジトリから読む
    let home = roots::data_dir()?;

    // 指示欄は短く保ち（上限は非公開。目安 5,000 字）、スキル本文と書式はプロジェクトファイルに回す
    let mut instructions = fs::read_to_string(root.join("AGENTS.md"))?;
    instructions.push_str(
        "\n## ChatGPT での使い方\n\n\
         - ナレッジはプロジェクトファイルの `knowledge-<領域>.md`（領域ごと）、`knowledge-projects.md`（プロダクト概要）、\
         `projects.csv`・`domains.csv` にある。各ノートの先頭の `<!-- path: ... -->` が元のパス\n\
         - 書式（`references/kb-format.md`）とスキルの手順は `skills.md` にある。依頼が下のスキルに当てはまったら、`skills.md` の該当節に従う\n\
         - CLI とファイル書き込みは使えない。記録を頼まれたら保存先のパスとフロントマター付きの本文を出力する\n\n\
         ## スキル\n\n",
    );
    let mut skills_md =
        String::from("# ika スキルと書式\n\n## 書式（references/kb-format.md）\n\n");
    skills_md.push_str(frontmatter::body(&fs::read_to_string(
        root.join("docs/kb-format.md"),
    )?));
    for dir in skills::dirs(root)? {
        let text = fs::read_to_string(dir.join("SKILL.md"))?;
        let name = dir.file_name().expect("skills/<name>").to_string_lossy();
        let description = frontmatter::parse(&text)?
            .and_then(|f| f.get("description").cloned())
            .unwrap_or_default();
        instructions.push_str(&format!("- `{name}`: {description}\n"));
        skills_md.push_str(&format!(
            "\n\n---\n\n# スキル: {name}\n\n{}",
            frontmatter::body(&text)
        ));
    }
    fs::write(out.join("instructions.md"), instructions)?;
    fs::write(out.join("skills.md"), skills_md)?;

    let mut written = vec!["instructions.md".to_string(), "skills.md".to_string()];
    written.extend(write_knowledge(&home, &out, scope)?);

    for name in written {
        println!("dist/chatgpt/{name}");
    }
    Ok(())
}

/// 領域ごと（kb/<領域>/）とプロダクト概要（kb/projects/）でファイルを分けて、`out` に書く。
/// 各ノートの先頭に元のパスを `<!-- path: ... -->` で付ける。書いたファイル名を返す
fn write_knowledge(home: &Path, out: &Path, scope: &Scope) -> Result<Vec<String>> {
    let domains = registry::domains(home)?;
    for name in &scope.domains {
        if !domains.iter().any(|d| &d.name == name) {
            bail!("未登録の領域です: {name}（`ika domain list` で確認してください）");
        }
    }
    let selected: Vec<_> = domains
        .into_iter()
        .filter(|d| scope.domains.is_empty() || scope.domains.contains(&d.name))
        .collect();

    remove_knowledge(out)?;
    let mut written = Vec::new();
    let groups = selected
        .iter()
        .map(|d| d.name.clone())
        .chain(scope.projects.then(|| "projects".to_string()));
    for group in groups {
        let dir = home.join("kb").join(&group);
        let mut files = Vec::new();
        if dir.is_dir() {
            check::collect_markdown(&dir, &mut files)?;
        }
        files.sort();
        let mut knowledge = format!("# ika ナレッジ: {group}\n");
        for file in &files {
            let rel = file.strip_prefix(home).unwrap_or(file);
            let text = fs::read_to_string(file)
                .with_context(|| format!("{} を読めません", rel.display()))?;
            knowledge.push_str(&format!(
                "\n\n<!-- path: {} -->\n\n{}",
                rel.display(),
                text.trim_end()
            ));
        }
        let name = format!("knowledge-{group}.md");
        fs::write(out.join(&name), knowledge)?;
        written.push(name);
    }

    // 登録簿。範囲を絞ったときは、絞った領域・プロダクトの行だけにする
    if scope.projects {
        fs::copy(home.join(registry::PROJECTS), out.join("projects.csv"))?;
        written.push("projects.csv".to_string());
    }
    if scope.domains.is_empty() {
        fs::copy(home.join(registry::DOMAINS), out.join("domains.csv"))?;
    } else {
        registry::save(out, "domains.csv", &selected)?;
    }
    written.push("domains.csv".to_string());
    Ok(written)
}

/// 前回の `write_knowledge` が書いたファイルを消す
fn remove_knowledge(out: &Path) -> Result<()> {
    for entry in fs::read_dir(out)? {
        let path = entry?.path();
        let name = path.file_name().expect("entry").to_string_lossy();
        let ours = (name.starts_with("knowledge-") && name.ends_with(".md"))
            || name == "projects.csv"
            || name == "domains.csv";
        if ours && path.is_file() {
            fs::remove_file(&path)?;
        }
    }
    Ok(())
}
