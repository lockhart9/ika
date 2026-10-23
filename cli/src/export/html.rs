//! `ika export html`: KB のページを、図と表の入った単独の HTML にする。
//! 出力は `IKA_HOME/.cache/html/` に KB と同じ階層で書く（Git に入れない。Markdown からいつでも作り直せる）。
//! CSS はインライン、画像は data URI で埋め込み、JavaScript と外部への参照は入れない。
//! 図の記法に誤りがあれば、1ファイルも書かずに止める

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::registry::{self, Project};
use crate::render::{
    self, Href, LinkKind, RenderOptions, Rendered, esc, is_web_url, percent_decode, percent_encode,
};
use crate::{check, frontmatter, import, kb, root, serve};

pub struct Request {
    pub path: Option<String>,
    pub project: Option<String>,
    pub all: bool,
    pub open: bool,
}

/// 見た目の上書き（IKA_HOME から）。既定のスタイルのあとに読み込む
const THEME: &str = "theme/style.css";

/// スクリプトと外部の読み込みを一切許さない。画像は埋め込んだ data URI だけ
const CSP: &str = "default-src 'none'; style-src 'unsafe-inline'; img-src data:";

const IMAGE_TYPES: &[(&str, &str)] = &[
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
];

/// ページの骨格（左の欄・本文）。色・フォントの変数は ika-diagram の `:root` と、ここで足す分
const PAGE_CSS: &str = r#"
/* 本文の要素の規則は :where で詳細度を下げ、図の部品（.ikd-*）の規則が勝つようにする */
:root { --code-bg:#ebeeec;
  --serif:"Hiragino Mincho ProN","Hiragino Mincho Pro","YuMincho","Yu Mincho","Noto Serif JP","Source Han Serif JP",serif; }
@media (prefers-color-scheme: dark) { :root { --code-bg:#1d2528; } }
* { box-sizing:border-box; }
html { font-size:17px; scroll-behavior:smooth; -webkit-text-size-adjust:100%; }
@media (prefers-reduced-motion: reduce) { html { scroll-behavior:auto; } }
body { margin:0; background:var(--paper); color:var(--ink); font-family:var(--serif); line-height:2;
  line-break:strict; overflow-wrap:anywhere; text-rendering:optimizeLegibility; }
a { color:var(--accent); text-decoration:underline; text-decoration-thickness:1px; text-underline-offset:.22em; }
a:focus-visible, summary:focus-visible { outline:2px solid var(--accent); outline-offset:3px; border-radius:2px; }
code, pre { font-family:var(--mono); }

.page { display:grid; grid-template-columns:12.5rem minmax(0,56rem); column-gap:3.5rem; max-width:76rem; margin:0 auto; padding:5.5rem 2rem 9rem; }
.rail { grid-column:1; grid-row:1 / span 2; font-family:var(--sans); font-size:.8rem; line-height:1.75; color:var(--ink-2); }
.rail-in { position:sticky; top:2.5rem; max-height:calc(100vh - 5rem); overflow-y:auto; }
.head { grid-column:2; grid-row:1; margin-bottom:1rem; }
.prose { grid-column:2; grid-row:2; display:grid; grid-template-columns:minmax(0,40rem) 1fr; align-content:start; }
.prose > *, .doc > * { grid-column:1; }
/* 図と表は右の余白まで広げる。min-width:0 で、中身の幅を列の幅の計算に入れない（入れると本文の列がつぶれる） */
.prose > :is(.ikd-fig, .ikd-error, table, .doc), .doc > :is(.ikd-fig, .ikd-error, table) { grid-column:1 / -1; min-width:0; }
.doc { display:grid; grid-template-columns:subgrid; }
:is(.prose, .doc) > :is(h1, h2, h3, h4) + :is(.ikd-fig, .ikd-error) { margin-top:1.2rem; }

.meta { margin:0 0 2.4rem; }
.meta dt { color:var(--ink-3); font-size:.72rem; margin-top:.9rem; }
.meta dt:first-child { margin-top:0; }
.meta dd { margin:0; color:var(--ink); }
.st { display:inline-block; margin-left:.3rem; padding:0 .55rem; border:1px solid var(--line-2); color:var(--ink-2); border-radius:1rem; font:.74rem/1.6 var(--sans); }
.st-open { border-color:var(--gold); color:var(--gold); }
.st-ok { border-color:var(--moss); color:var(--moss); }
.st-ended { color:var(--ink-3); }
.toc ol { list-style:none; margin:0; padding:0; border-left:1px solid var(--line-2); }
.toc ol ol { border-left:0; margin-bottom:.4rem; }
.toc li { margin:0; }
.toc a, .toc span { display:block; padding:.28rem 0 .28rem .9rem; margin-left:-1px; border-left:2px solid transparent; color:var(--ink-2); text-decoration:none; }
.toc ol ol a { padding:.15rem 0 .15rem 1.6rem; font-size:.76rem; color:var(--ink-3); }
.toc span { color:var(--ink); font-weight:700; }
.toc a:hover { color:var(--accent); border-left-color:var(--accent); }

.crumbs { font:.76rem/1.6 var(--sans); color:var(--ink-3); margin:0 0 2.2rem; }
.crumbs i { font-style:normal; margin:0 .35em; color:var(--line-2); }
h1, h2, h3, h4, h5, h6 { font-family:var(--sans); font-feature-settings:"palt"; text-wrap:balance; scroll-margin-top:2rem; }
.head h1 { font-weight:800; font-size:clamp(2rem,4.4vw,3rem); line-height:1.38; letter-spacing:-.01em; margin:0 0 1.8rem; }
.lede { font-size:1.2rem; line-height:2; margin:0 0 .5rem; max-width:40rem; text-wrap:pretty; }
:where(.prose) h1 { font-size:1.9rem; line-height:1.45; margin:4rem 0 1.2rem; }
:where(.prose) h2 { font-size:1.6rem; line-height:1.5; margin:5rem 0 1.3rem; }
:where(.prose) h3 { font-size:1.15rem; line-height:1.6; margin:2.8rem 0 .6rem; }
:where(.prose) h4, :where(.prose) h5, :where(.prose) h6 { font-size:1rem; margin:2rem 0 .4rem; }
.prose > :first-child { margin-top:0; }
:where(.prose) p { margin:0 0 1.35em; text-wrap:pretty; }
:where(.prose) ul, :where(.prose) ol { margin:0 0 1.5em; padding-left:1.4em; }
:where(.prose) li { margin:.25em 0; } :where(.prose) li > p { margin:0; }
:where(.prose) ul { list-style:square; } :where(.prose) li::marker { color:var(--ink-3); }
:where(.prose) input[type=checkbox] { accent-color:var(--moss); margin:0 .5em 0 0; }
:where(.prose) :not(pre) > code { font-size:.84em; background:var(--code-bg); padding:.12em .4em; border-radius:4px; }
:where(.prose) blockquote { margin:2rem 0; padding:0 0 0 1.6rem; border-left:3px solid var(--line-2); color:var(--ink-2); }
:where(.prose) pre { margin:1.8rem 0; padding:1rem 1.3rem; background:var(--code-bg); border-radius:6px; overflow-x:auto; font-size:.8rem; line-height:1.85; tab-size:2; }
:where(.prose) img { max-width:100%; height:auto; }
:where(.prose) hr { border:0; border-top:1px solid var(--line); margin:3rem 0; }
:where(.prose) table { display:block; overflow-x:auto; border-collapse:collapse; margin:2rem 0; font:.86rem/1.75 var(--sans); }
:where(.prose) th { text-align:left; font-weight:700; padding:.5rem .9rem .5rem 0; border-bottom:1.5px solid var(--ink); white-space:nowrap; }
:where(.prose) td { padding:.6rem .9rem .6rem 0; border-bottom:1px solid var(--line); vertical-align:top; min-width:6rem; }
:where(.prose) .footnote-definition { font-size:.85rem; color:var(--ink-2); }

.part { font:700 .95rem/1.6 var(--sans); color:var(--accent); margin:6rem 0 0 !important; padding-top:1rem; border-top:1px solid var(--line-2); scroll-margin-top:2rem; }
.prose > .part:first-child { margin-top:0 !important; }
.doc { margin-top:3.5rem; }
.doc .doc-title { margin:0 0 .4rem; font-size:1.6rem; line-height:1.5; }
.doc-meta { font:.78rem/1.7 var(--sans) !important; color:var(--ink-3); margin:0 0 1.6rem !important; }
.doc-meta .st { margin:0 .5rem 0 0; }
details.ended { margin-top:3rem; }
details.ended > summary { cursor:pointer; font:700 .95rem/1.6 var(--sans); color:var(--ink-2); }
details.ended .doc { display:block; opacity:.85; }
.doclist { list-style:none !important; padding:0 !important; font-family:var(--sans); font-size:.9rem; line-height:1.7; }
.doclist li { padding:.45rem 0; border-bottom:1px solid var(--line); margin:0 !important; }
.doclist .date { margin-left:.6rem; font-size:.78rem; color:var(--ink-3); }

@media (max-width:64rem) {
  .page { grid-template-columns:minmax(0,1fr); padding:2.5rem 1.2rem 6rem; }
  .rail, .head, .prose { grid-column:1; }
  .head { grid-row:1; } .rail { grid-row:2; margin:1rem 0 2.5rem; } .prose { grid-row:3; grid-template-columns:minmax(0,1fr); }
  .rail-in { position:static; max-height:none; }
  :where(.prose) h2 { margin-top:4rem; }
}
@media print {
  :root { --paper:#fff; --raise:#fff; }
  .page { display:block; padding:0; max-width:none; } .rail-in { position:static; } .toc { display:none; }
  details.ended { display:block; }
}
"#;

/// kb/ の Markdown 1件
struct Doc {
    rel: PathBuf,
    text: String,
    title: String,
    kind: String,
    status: String,
    date: String,
    projects: Vec<String>,
    tags: Vec<String>,
    source: String,
}

impl Doc {
    fn html_rel(&self) -> PathBuf {
        self.rel.with_extension("html")
    }

    fn body(&self) -> &str {
        frontmatter::body(&self.text)
    }

    /// 本文の1行目の、ファイルの中の行番号から 1 を引いた数
    fn body_offset(&self) -> usize {
        frontmatter::head(&self.text).matches('\n').count()
    }
}

fn load(home: &Path) -> Result<Vec<Doc>> {
    let mut files = Vec::new();
    check::collect_markdown(&home.join("kb"), &mut files)?;
    files.sort();
    let mut docs = Vec::new();
    for file in files {
        let rel = file.strip_prefix(home).unwrap_or(&file).to_path_buf();
        let text =
            fs::read_to_string(&file).with_context(|| format!("読めません: {}", rel.display()))?;
        let fields = frontmatter::parse(&text).ok().flatten().unwrap_or_default();
        let field = |key: &str| fields.get(key).cloned().unwrap_or_default();
        let list = |key: &str| {
            fields
                .get(key)
                .and_then(|v| frontmatter::list(v).ok())
                .unwrap_or_default()
        };
        let title = Some(field("title"))
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| rel.file_stem().unwrap_or_default().to_string_lossy().into());
        docs.push(Doc {
            title,
            kind: field("type"),
            status: field("status"),
            date: field("date"),
            projects: list("projects"),
            tags: list("tags"),
            source: field("source"),
            text,
            rel,
        });
    }
    Ok(docs)
}

pub fn run(home: &Path, req: Request) -> Result<()> {
    let modes = [req.path.is_some(), req.project.is_some(), req.all];
    if modes.iter().filter(|m| **m).count() != 1 {
        bail!(
            "書き出すものを1つ指定してください: <kb/... のパス>・--project <name>・--all のどれか"
        );
    }
    let docs = load(home)?;
    let out = root::cache_dir(home)?.join("html");
    let mut errors = Vec::new();
    let mut pages: Vec<(PathBuf, String)> = Vec::new();

    if let Some(path) = &req.path {
        let rel = kb::resolve(home, path)?;
        kb::guard_symlink(home, &rel)?;
        if rel.extension().is_none_or(|ext| ext != "md") {
            bail!("Markdown（.md）のページを指定してください: {path}");
        }
        let doc = docs
            .iter()
            .find(|d| d.rel == rel)
            .with_context(|| format!("ファイルがありません: {}", rel.display()))?;
        let export = Export::new(home, &out, &docs, HashSet::from([rel.clone()]))?;
        pages.push((doc.html_rel(), export.page(doc, &mut errors)));
    } else if let Some(name) = &req.project {
        let projects = registry::projects(home)?;
        let project = projects.iter().find(|p| &p.name == name).with_context(|| {
            format!("未登録のプロダクトです: {name}（`ika project list` で確認してください）")
        })?;
        let export = Export::new(home, &out, &docs, HashSet::new())?;
        pages.push((bundle_rel(name), export.bundle(project, &mut errors)));
    } else {
        let planned = docs.iter().map(|d| d.rel.clone()).collect();
        let export = Export::new(home, &out, &docs, planned)?;
        for doc in &docs {
            pages.push((doc.html_rel(), export.page(doc, &mut errors)));
        }
        pages.push((PathBuf::from("index.html"), export.index()?));
    }

    if !errors.is_empty() {
        bail!(
            "図の記法に誤りがあるので、書き出しませんでした（`ika check` でも確かめられます）:\n{}",
            errors.join("\n")
        );
    }
    for (rel, html) in &pages {
        let path = out.join(rel);
        fs::create_dir_all(path.parent().expect("out の下"))?;
        fs::write(&path, html).with_context(|| format!("書けません: {}", path.display()))?;
    }
    let shown = if req.all {
        let index = out.join("index.html");
        println!(
            "{} ページを書き出しました: {}",
            pages.len() - 1,
            index.display()
        );
        index
    } else {
        let path = out.join(&pages[0].0);
        println!("{}", path.display());
        path
    };
    if req.open {
        serve::open_browser(&shown.to_string_lossy());
    }
    Ok(())
}

fn bundle_rel(name: &str) -> PathBuf {
    PathBuf::from(format!("project-{name}.html"))
}

/// 1回の書き出し
struct Export<'a> {
    home: &'a Path,
    out: &'a Path,
    docs: &'a [Doc],
    /// この回で書くページ（ルートからの相対の .md）。リンクを相対リンクにしてよい
    planned: HashSet<PathBuf>,
    /// リンク先 → リンク元
    inbound: HashMap<PathBuf, Vec<usize>>,
    theme: String,
}

/// 束ねた資料の中のページ → アンカー（`d3`）。中の見出しは `d3-見出し`
type Anchors = HashMap<PathBuf, String>;

impl<'a> Export<'a> {
    fn new(
        home: &'a Path,
        out: &'a Path,
        docs: &'a [Doc],
        planned: HashSet<PathBuf>,
    ) -> Result<Self> {
        let mut inbound: HashMap<PathBuf, Vec<usize>> = HashMap::new();
        for (i, doc) in docs.iter().enumerate() {
            let dir = doc.rel.parent().unwrap_or(Path::new(""));
            let mut seen = HashSet::new();
            for link in check::link_targets(&doc.text) {
                let target = kb::normalize(&dir.join(link));
                if target != doc.rel && seen.insert(target.clone()) {
                    inbound.entry(target).or_default().push(i);
                }
            }
        }
        let theme = match fs::read_to_string(home.join(THEME)) {
            // 自分の CSS でも、<style> を閉じられないようにする
            Ok(css) => css.replace("</", "<\\/"),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(err) => return Err(err).context(THEME),
        };
        Ok(Export {
            home,
            out,
            docs,
            planned,
            inbound,
            theme,
        })
    }

    fn layout(&self, title: &str, rail: &str, head: &str, prose: &str) -> String {
        format!(
            "<!doctype html><html lang=\"ja\"><head><meta charset=\"utf-8\">\
             <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
             <meta http-equiv=\"Content-Security-Policy\" content=\"{CSP}\">\
             <title>{}</title><style>{}{PAGE_CSS}{}{}</style></head><body><div class=\"page\">\
             <aside class=\"rail\"><div class=\"rail-in\">{rail}</div></aside>\
             <header class=\"head\">{head}</header><article class=\"prose\">{prose}</article>\
             </div></body></html>\n",
            esc(title),
            ika_diagram::CSS,
            render::CALLOUT_CSS,
            self.theme
        )
    }

    /// `doc` の本文を、`from`（出力先からの相対の HTML）に置くものとして描く
    fn render(
        &self,
        doc: &Doc,
        from: &Path,
        anchors: Option<&Anchors>,
        prefix: &str,
        errors: &mut Vec<String>,
    ) -> Rendered {
        let link = |kind: LinkKind, url: &str| self.href(kind, url, doc, from, anchors, prefix);
        let rendered = render::markdown(
            doc.body(),
            &RenderOptions {
                link: &link,
                diagrams: !import::is_source_path(&doc.rel),
                id_prefix: prefix,
                heading_shift: u8::from(anchors.is_some()),
            },
        );
        for error in &rendered.errors {
            errors.push(format!(
                "{}: {}行目: {}",
                doc.rel.display(),
                error.line + doc.body_offset(),
                error.message
            ));
        }
        rendered
    }

    /// リンク・画像の行き先。KB の中のページは、束ねた中ならアンカー、書き出した（書き出す）HTML があれば相対リンク、
    /// なければリンクを外してパスを添える。画像は data URI で埋め込み、外部の画像はリンクにする
    fn href(
        &self,
        kind: LinkKind,
        url: &str,
        doc: &Doc,
        from: &Path,
        anchors: Option<&Anchors>,
        prefix: &str,
    ) -> Href {
        if let Some(fragment) = url.strip_prefix('#') {
            if fragment.is_empty() {
                return Href::Url("#".into());
            }
            return Href::Url(format!("#{prefix}{}", fragment_id(fragment)));
        }
        if url.starts_with("mailto:") {
            return Href::Url(url.into());
        }
        if is_web_url(url) {
            return match kind {
                LinkKind::Link => Href::Url(url.into()),
                LinkKind::Image => Href::LinkTo(url.into()),
            };
        }
        let (path, fragment) = url.split_once('#').unwrap_or((url, ""));
        let path = path.split('?').next().unwrap_or_default();
        let Some(target) = percent_decode(path, false)
            .and_then(|decoded| resolve(doc.rel.parent().unwrap_or(Path::new("")), &decoded))
        else {
            return Href::Text(url.into());
        };
        let shown = target.to_string_lossy().into_owned();
        if !target.starts_with("kb") || path.ends_with('/') {
            return Href::Text(shown);
        }
        match kind {
            LinkKind::Image => match self.data_uri(&target) {
                Some(uri) => Href::Url(uri),
                None => Href::Text(shown),
            },
            LinkKind::Link if target.extension().is_some_and(|ext| ext == "md") => {
                if let Some(anchor) = anchors.and_then(|a| a.get(&target)) {
                    return Href::Url(if fragment.is_empty() {
                        format!("#{anchor}")
                    } else {
                        format!("#{anchor}-{}", fragment_id(fragment))
                    });
                }
                let html = target.with_extension("html");
                if self.planned.contains(&target) || self.out.join(&html).is_file() {
                    let fragment = if fragment.is_empty() {
                        String::new()
                    } else {
                        format!("#{}", percent_encode(&fragment_id(fragment)))
                    };
                    Href::Url(format!("{}{fragment}", relative(from, &html)))
                } else {
                    Href::Text(shown)
                }
            }
            LinkKind::Link => Href::Text(shown),
        }
    }

    fn data_uri(&self, target: &Path) -> Option<String> {
        let ext = target.extension()?.to_string_lossy().to_ascii_lowercase();
        let (_, mime) = IMAGE_TYPES.iter().find(|(e, _)| *e == ext)?;
        kb::guard_symlink(self.home, target).ok()?;
        let bytes = fs::read(self.home.join(target)).ok()?;
        Some(format!("data:{mime};base64,{}", base64(&bytes)))
    }

    /// KB のページ（`to` はルートからの相対の .md）への行き先。書き出していなければ文字だけ
    fn doc_link(&self, from: &Path, to: &Path, label: &str) -> String {
        let html = to.with_extension("html");
        if self.planned.contains(to) || self.out.join(&html).is_file() {
            format!(
                "<a href=\"{}\">{}</a>",
                esc(&relative(from, &html)),
                esc(label)
            )
        } else {
            esc(label)
        }
    }

    fn meta(&self, doc: &Doc, from: &Path) -> String {
        let mut rows = vec![(
            "種類",
            format!(
                "{}{}",
                esc(kind_label(&doc.kind)),
                status_badge(&doc.status)
            ),
        )];
        if !doc.date.is_empty() {
            rows.push(("日付", esc(&doc.date)));
        }
        if !doc.projects.is_empty() {
            let links: Vec<_> = doc
                .projects
                .iter()
                .map(|p| self.doc_link(from, &registry::readme_path(Path::new(""), p), p))
                .collect();
            rows.push(("プロダクト", links.join("、")));
        }
        if !doc.tags.is_empty() {
            let tags: Vec<_> = doc.tags.iter().map(|t| esc(t)).collect();
            rows.push(("タグ", tags.join("、")));
        }
        if is_web_url(&doc.source) {
            rows.push(("出典", format!("<a href=\"{0}\">{0}</a>", esc(&doc.source))));
        } else if !doc.source.is_empty() {
            rows.push(("出典", esc(&doc.source)));
        }
        let mut out = String::from("<dl class=\"meta\">");
        for (label, value) in rows {
            out.push_str(&format!("<dt>{label}</dt><dd>{value}</dd>"));
        }
        out.push_str("</dl>");
        out
    }

    /// 1ページ
    fn page(&self, doc: &Doc, errors: &mut Vec<String>) -> String {
        let from = doc.html_rel();
        let rendered = self.render(doc, &from, None, "", errors);
        let mut toc = String::new();
        for heading in rendered.headings.iter().filter(|h| h.level == 2) {
            toc.push_str(&format!(
                "<li><a href=\"#{}\">{}</a></li>",
                esc(&heading.id),
                esc(&heading.text)
            ));
        }
        let mut prose = rendered.html;
        if let Some(referrers) = self.inbound.get(&doc.rel) {
            prose.push_str("<h2 id=\"inbound\">このページへのリンク</h2><ul class=\"doclist\">");
            for &i in referrers {
                let referrer = &self.docs[i];
                prose.push_str(&format!(
                    "<li>{}<span class=\"date\">{}</span></li>",
                    self.doc_link(&from, &referrer.rel, &referrer.title),
                    esc(&referrer.date)
                ));
            }
            prose.push_str("</ul>");
            toc.push_str("<li><a href=\"#inbound\">このページへのリンク</a></li>");
        }
        let rail = format!(
            "{}{}",
            self.meta(doc, &from),
            if toc.is_empty() {
                String::new()
            } else {
                format!("<nav class=\"toc\" aria-label=\"目次\"><ol>{toc}</ol></nav>")
            }
        );
        let head = format!(
            "{}<h1>{}</h1>",
            crumbs(doc.rel.parent().unwrap_or(Path::new(""))),
            esc(&doc.title)
        );
        self.layout(&doc.title, &rail, &head, &prose)
    }

    /// プロダクトの README・資料・要件書・決定・知見を1ファイルに束ねる
    fn bundle(&self, project: &Project, errors: &mut Vec<String>) -> String {
        let name = &project.name;
        let from = bundle_rel(name);
        let base = registry::readme_path(Path::new(""), name)
            .parent()
            .expect("kb/projects/<name>")
            .to_path_buf();
        let readme = base.join("README.md");
        let specs_dir = base.join("specs");
        let mine = |d: &&Doc| d.projects.iter().any(|p| p == name);

        let readme_docs: Vec<&Doc> = self.docs.iter().filter(|d| d.rel == readme).collect();
        let mut materials: Vec<&Doc> = self
            .docs
            .iter()
            .filter(|d| {
                d.rel.starts_with(&base)
                    && d.rel != readme
                    && !d.rel.starts_with(&specs_dir)
                    && !import::is_source_path(&d.rel)
            })
            .collect();
        materials.sort_by(|a, b| a.rel.cmp(&b.rel));
        let specs = self
            .docs
            .iter()
            .filter(|d| d.kind == "spec" && d.rel.starts_with(&specs_dir));
        let decisions = self
            .docs
            .iter()
            .filter(|d| d.kind == "decision")
            .filter(mine);
        let ended_status =
            |d: &&Doc| matches!(d.status.as_str(), "dropped" | "superseded" | "rejected");
        let mut ended: Vec<&Doc> = specs
            .clone()
            .chain(decisions.clone())
            .filter(ended_status)
            .collect();
        let mut live_specs: Vec<&Doc> = specs.filter(|d| !ended_status(d)).collect();
        live_specs.sort_by_key(|d| (rank(&d.status, &["approved", "draft", "done"]), newest(d)));
        let mut live_decisions: Vec<&Doc> = decisions.filter(|d| !ended_status(d)).collect();
        live_decisions.sort_by_key(|d| (rank(&d.status, &["accepted", "proposed"]), newest(d)));
        let mut insights: Vec<&Doc> = self
            .docs
            .iter()
            .filter(|d| d.kind == "insight")
            .filter(mine)
            .collect();
        insights.sort_by_key(|d| newest(d));
        ended.sort_by_key(|d| newest(d));
        let mut notes: Vec<&Doc> = self
            .docs
            .iter()
            .filter(|d| d.kind == "note")
            .filter(mine)
            .collect();
        notes.sort_by_key(|d| newest(d));

        let groups: [(&str, &str, &[&Doc]); 5] = [
            ("readme", "概要", &readme_docs),
            ("materials", "資料", &materials),
            ("specs", "要件書", &live_specs),
            ("decisions", "決定", &live_decisions),
            ("insights", "知見", &insights),
        ];
        let mut anchors = Anchors::new();
        for doc in groups
            .iter()
            .flat_map(|(_, _, docs)| docs.iter())
            .chain(&ended)
        {
            let id = format!("d{}", anchors.len() + 1);
            anchors.insert(doc.rel.clone(), id);
        }

        let section = |doc: &Doc, errors: &mut Vec<String>| {
            let anchor = &anchors[&doc.rel];
            let rendered = self.render(doc, &from, Some(&anchors), &format!("{anchor}-"), errors);
            format!(
                "<section class=\"doc\" id=\"{anchor}\"><h2 class=\"doc-title\">{}</h2>\
                 <p class=\"doc-meta\">{}{}<span class=\"kbpath\">{}</span></p>{}</section>",
                esc(&doc.title),
                status_badge(&doc.status),
                esc(&doc.date),
                esc(&doc.rel.to_string_lossy()),
                rendered.html
            )
        };
        let mut prose = String::new();
        let mut toc = String::new();
        for (id, label, docs) in groups {
            if docs.is_empty() {
                continue;
            }
            prose.push_str(&format!("<p class=\"part\" id=\"g-{id}\">{label}</p>"));
            toc.push_str(&format!("<li><a href=\"#g-{id}\">{label}</a><ol>"));
            for doc in docs {
                prose.push_str(&section(doc, errors));
                toc.push_str(&format!(
                    "<li><a href=\"#{}\">{}</a></li>",
                    anchors[&doc.rel],
                    esc(&doc.title)
                ));
            }
            toc.push_str("</ol></li>");
        }
        if !ended.is_empty() {
            prose.push_str(&format!(
                "<details class=\"ended\" id=\"g-ended\"><summary>終わったもの（{} 件）</summary>",
                ended.len()
            ));
            for doc in &ended {
                prose.push_str(&section(doc, errors));
            }
            prose.push_str("</details>");
            toc.push_str("<li><a href=\"#g-ended\">終わったもの</a></li>");
        }
        if !notes.is_empty() {
            prose.push_str("<p class=\"part\" id=\"g-notes\">関連ノート</p><ul class=\"doclist\">");
            for doc in &notes {
                prose.push_str(&format!(
                    "<li>{}<span class=\"date\">{}</span></li>",
                    self.doc_link(&from, &doc.rel, &doc.title),
                    esc(&doc.date)
                ));
            }
            prose.push_str("</ul>");
            toc.push_str("<li><a href=\"#g-notes\">関連ノート</a></li>");
        }

        let mut rail = String::from("<dl class=\"meta\"><dt>種類</dt><dd>プロダクトの資料</dd>");
        rail.push_str(&format!("<dt>状態</dt><dd>{}</dd>", esc(&project.status)));
        if let Some(doc) = readme_docs.first() {
            rail.push_str(&format!(
                "<dt>README の日付</dt><dd>{}</dd>",
                esc(&doc.date)
            ));
        }
        rail.push_str(&format!(
            "<dt>収めたページ</dt><dd>{} 件</dd></dl><nav class=\"toc\" aria-label=\"目次\"><ol>{toc}</ol></nav>",
            anchors.len()
        ));
        let lede = if project.summary.is_empty() {
            String::new()
        } else {
            format!("<p class=\"lede\">{}</p>", esc(&project.summary))
        };
        let head = format!("{}<h1>{}</h1>{lede}", crumbs(&base), esc(name));
        self.layout(name, &rail, &head, &prose)
    }

    /// `--all` の入口。ディレクトリごとにページを並べる
    fn index(&self) -> Result<String> {
        let from = Path::new("index.html");
        let mut prose = String::new();
        let mut toc = String::new();
        let bundles: Vec<_> = registry::projects(self.home)?
            .into_iter()
            .filter(|p| self.out.join(bundle_rel(&p.name)).is_file())
            .collect();
        if !bundles.is_empty() {
            prose.push_str(
                "<p class=\"part\" id=\"g-bundles\">プロダクトの資料</p><ul class=\"doclist\">",
            );
            for p in &bundles {
                prose.push_str(&format!(
                    "<li><a href=\"{}\">{}</a><span class=\"date\">{}</span></li>",
                    esc(&relative(from, &bundle_rel(&p.name))),
                    esc(&p.name),
                    esc(&p.summary)
                ));
            }
            prose.push_str("</ul>");
            toc.push_str("<li><a href=\"#g-bundles\">プロダクトの資料</a></li>");
        }
        let mut dirs: BTreeMap<PathBuf, Vec<&Doc>> = BTreeMap::new();
        for doc in self.docs {
            dirs.entry(doc.rel.parent().unwrap_or(Path::new("")).to_path_buf())
                .or_default()
                .push(doc);
        }
        for (i, (dir, docs)) in dirs.iter_mut().enumerate() {
            docs.sort_by_key(|d| newest(d));
            let label = dir.to_string_lossy();
            prose.push_str(&format!(
                "<p class=\"part\" id=\"dir{i}\">{}</p><ul class=\"doclist\">",
                esc(&label)
            ));
            for doc in docs.iter() {
                prose.push_str(&format!(
                    "<li>{}<span class=\"date\">{}</span></li>",
                    self.doc_link(from, &doc.rel, &doc.title),
                    esc(&doc.date)
                ));
            }
            prose.push_str("</ul>");
            toc.push_str(&format!("<li><a href=\"#dir{i}\">{}</a></li>", esc(&label)));
        }
        let rail = format!(
            "<dl class=\"meta\"><dt>ページ</dt><dd>{} 件</dd></dl><nav class=\"toc\" aria-label=\"目次\"><ol>{toc}</ol></nav>",
            self.docs.len()
        );
        Ok(self.layout("KB", &rail, "<h1>KB</h1>", &prose))
    }
}

/// 状態の並び順（`order` にないものは最後）
fn rank(status: &str, order: &[&str]) -> usize {
    order
        .iter()
        .position(|s| *s == status)
        .unwrap_or(order.len())
}

/// 新しい日付が先、同じならパス順
fn newest(doc: &Doc) -> (std::cmp::Reverse<String>, PathBuf) {
    (std::cmp::Reverse(doc.date.clone()), doc.rel.clone())
}

fn kind_label(kind: &str) -> &str {
    match kind {
        "project" => "プロダクト概要",
        "spec" => "要件書",
        "note" => "ノート",
        "insight" => "知見",
        "decision" => "決定",
        "source" => "取り込んだ資料",
        other => other,
    }
}

fn status_badge(status: &str) -> String {
    let (label, tone) = match status {
        "" => return String::new(),
        "draft" => ("下書き", "open"),
        "proposed" => ("案", "open"),
        "approved" => ("承認済み", "ok"),
        "accepted" => ("採用", "ok"),
        "done" => ("実装済み", "ok"),
        "superseded" => ("覆された", "ended"),
        "rejected" => ("却下", "ended"),
        "dropped" => ("中止", "ended"),
        other => (other, ""),
    };
    format!("<span class=\"st st-{tone}\">{}</span>", esc(label))
}

fn crumbs(dir: &Path) -> String {
    let parts: Vec<_> = dir.iter().map(|p| esc(&p.to_string_lossy())).collect();
    format!("<p class=\"crumbs\">{}</p>", parts.join("<i>/</i>"))
}

/// リンクの `#` 以降を、見出しの id と同じ形にする
fn fragment_id(fragment: &str) -> String {
    render::slug(&percent_decode(fragment, false).unwrap_or_else(|| fragment.to_string()))
}

/// `dir`（ルートからの相対）を基準に `link` を解決する。ルートより上に出たら `None`
fn resolve(dir: &Path, link: &str) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for component in dir.join(link).components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return None;
                }
            }
            _ => return None,
        }
    }
    Some(out)
}

/// 出力先の中で、`from`（HTML）から `to` への相対 URL
fn relative(from: &Path, to: &Path) -> String {
    let from_dir: Vec<_> = from.parent().unwrap_or(Path::new("")).iter().collect();
    let to_parts: Vec<_> = to.iter().collect();
    let common = from_dir
        .iter()
        .zip(&to_parts)
        .take_while(|(a, b)| a == b)
        .count();
    let mut parts: Vec<String> = vec!["..".into(); from_dir.len() - common];
    parts.extend(
        to_parts[common..]
            .iter()
            .map(|p| p.to_string_lossy().into_owned()),
    );
    percent_encode(&parts.join("/"))
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(TABLE[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_climbs_out_of_the_source_directory() {
        assert_eq!(
            relative(
                Path::new("kb/tech/notes/2026/a.html"),
                Path::new("kb/projects/ika/README.html")
            ),
            "../../../projects/ika/README.html"
        );
        assert_eq!(
            relative(Path::new("project-ika.html"), Path::new("kb/tech/x y.html")),
            "kb/tech/x%20y.html"
        );
        assert_eq!(
            relative(Path::new("kb/a/b.html"), Path::new("kb/a/c.html")),
            "c.html"
        );
    }

    #[test]
    fn resolve_refuses_to_leave_the_root() {
        assert_eq!(
            resolve(Path::new("kb/projects/ika"), "../../tech/./a.md"),
            Some(PathBuf::from("kb/tech/a.md"))
        );
        assert_eq!(resolve(Path::new("kb"), "../../x.md"), None);
    }

    #[test]
    fn base64_matches_the_standard_alphabet_and_padding() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0xff, 0xfe]), "//4=");
    }

    #[test]
    fn status_badges_use_japanese_labels() {
        assert_eq!(
            status_badge("accepted"),
            "<span class=\"st st-ok\">採用</span>"
        );
        assert_eq!(status_badge(""), "");
    }
}
