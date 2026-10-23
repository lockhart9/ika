//! `ika kb fts`: 日本語の形態素解析（lindera・IPADIC）と tantivy で全文検索し、関連度順に並べる。
//!
//! 語は2通りで当てる。形態素（助詞・記号を除き、活用は基本形にそろえる）での一致と、
//! 2文字ずつ区切った索引での部分一致（`kb search` と同じく、語の途中でも当たる）。
//! 1文字の語は部分一致では引かず、形態素として当たるときだけ当たる。
//! 索引は `$IKA_HOME/.cache/fts/` に置いて git には入れず、検索のたびに変わったファイルだけ入れ直す
//! （`ika kb index` で更新だけ、`--rebuild` で作り直しもできる）。Markdown が正本で、索引は消してもいつでも作り直せる。

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::sync::OnceLock;
use std::time::{Duration, Instant, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use lindera::dictionary::load_dictionary;
use lindera::mode::Mode;
use lindera::segmenter::Segmenter;
use serde::{Deserialize, Serialize};
use tantivy::collector::{Count, TopDocs};
use tantivy::directory::error::LockError;
use tantivy::query::{
    AllQuery, BooleanQuery, BoostQuery, ConstScoreQuery, EmptyQuery, Occur, PhraseQuery, Query,
    TermQuery,
};
use tantivy::schema::{
    Field, IndexRecordOption, STORED, STRING, Schema, TextFieldIndexing, TextOptions, Value,
};
use tantivy::tokenizer::{Token, TokenStream, Tokenizer};
use tantivy::{Index, IndexWriter, ReloadPolicy, TantivyDocument, TantivyError, Term};

use super::search;
use crate::{check, frontmatter};

/// 索引の作り（フィールド・解析のしかた）を変えたら上げる。違えば作り直す
const VERSION: u32 = 2;
/// 1件あたりに表示する一致行の数
const MAX_LINES: usize = 3;

#[derive(clap::Args, Default)]
pub struct Args {
    /// 検索語（大文字小文字は区別しない。文でもよい。複数ならすべてに当たるファイルだけ）
    pub(crate) terms: Vec<String>,
    /// 領域（kb/<領域>/ に絞る）
    #[arg(short, long)]
    pub(crate) domain: Option<String>,
    /// プロダクト名（フロントマターの projects に含まれるもの）
    #[arg(short, long)]
    pub(crate) project: Option<String>,
    /// タグ（フロントマターの tags に含まれるもの）
    #[arg(short, long)]
    pub(crate) tag: Option<String>,
    /// 種類（project / spec / note / insight / decision / source）
    #[arg(long = "type")]
    pub(crate) kind: Option<String>,
    /// decision・spec の status
    #[arg(long)]
    pub(crate) status: Option<String>,
    /// 検索語のどれか1つに当たれば対象にする（表記ゆれ用）
    #[arg(long)]
    pub(crate) any: bool,
    /// ファイル名だけ表示する
    #[arg(short = 'l', long)]
    pub(crate) files: bool,
    /// 表示する件数（既定は検索語があれば 20、なければ全件）
    #[arg(short = 'n', long)]
    pub(crate) limit: Option<usize>,
}

/// 検索語があるときに表示する件数の既定
const DEFAULT_LIMIT: usize = 20;

/// 当たったファイル1件分
pub struct Hit {
    pub rel: String,
    pub date: String,
    /// 種類と status（`decision accepted` など）
    pub label: String,
    /// 検索語を含む行（行番号は 1 から）
    pub lines: Vec<(usize, String)>,
}

/// 検索の結果
pub struct Found {
    /// 当たった件数（表示するのは `hits` だけ）
    pub total: usize,
    pub hits: Vec<Hit>,
    /// 別の ika が索引を更新中で、更新前の索引で探した
    pub stale: bool,
}

/// 索引を更新してから探す。並びは関連度順（検索語がなければ新しい日付順）
pub fn find(root: &Path, args: &Args) -> Result<Found> {
    let (fts, stats) = Fts::open(root, false)?;
    let (total, rels) = fts.search(args)?;
    let needles = needles(&args.terms);
    let hits = rels
        .into_iter()
        .map(|rel| {
            let text = fs::read_to_string(root.join(&rel)).unwrap_or_default();
            let (_, date, label) = search::weight(&text);
            let lines = text
                .lines()
                .enumerate()
                .filter(|(_, line)| {
                    let line = line.to_lowercase();
                    needles.iter().any(|n| line.contains(n.as_str()))
                })
                .map(|(n, line)| (n + 1, line.trim().to_string()))
                .collect();
            Hit {
                rel,
                date,
                label,
                lines,
            }
        })
        .collect();
    Ok(Found {
        total,
        hits,
        stale: stats.is_none(),
    })
}

pub fn run(root: &Path, args: Args) -> Result<()> {
    let found = find(root, &args)?;
    if found.stale {
        eprintln!("索引を別の ika が更新中のため、更新前の索引で検索しました");
    }
    for hit in &found.hits {
        if args.files {
            println!("{}", hit.rel);
            continue;
        }
        println!("{}  [{}] {}", hit.rel, hit.label, hit.date);
        for (n, line) in hit.lines.iter().take(MAX_LINES) {
            println!("  {n}: {line}");
        }
        if hit.lines.len() > MAX_LINES {
            println!("  …ほか {} 行", hit.lines.len() - MAX_LINES);
        }
    }
    if found.hits.len() < found.total {
        eprintln!(
            "{} 件（上位 {} 件を表示。-n で増やせる）",
            found.total,
            found.hits.len()
        );
    } else {
        eprintln!("{} 件", found.total);
    }
    Ok(())
}

/// `ika kb index`: 索引に kb/ の変更を反映する（`rebuild` なら一から作り直す）
pub fn index(root: &Path, rebuild: bool) -> Result<()> {
    let started = Instant::now();
    let (fts, stats) = Fts::open(root, rebuild)?;
    let Some(stats) = stats else {
        bail!("索引を別の ika が更新中です。少し待ってからやり直してください");
    };
    let total = fts
        .index
        .reader_builder()
        .reload_policy(ReloadPolicy::Manual)
        .try_into()
        .map(|r: tantivy::IndexReader| r.searcher().num_docs())?;
    println!(
        "{}: 追加 {} · 更新 {} · 削除 {} · 索引 {total} 件（{:.2} 秒）",
        if rebuild {
            "作り直しました"
        } else {
            "更新しました"
        },
        stats.added,
        stats.updated,
        stats.removed,
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

/// 一致した行を探すための文字列（検索語そのものと、その形態素の表層形・基本形。1文字の形態素は除く）
pub(crate) fn needles(terms: &[String]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for term in terms {
        let lower = term.to_lowercase();
        for word in analyze(&lower) {
            for form in [word.surface, word.base] {
                if form.chars().count() >= 2 {
                    out.insert(form);
                }
            }
        }
        out.insert(lower);
    }
    out
}

// ---- 解析 ----

/// 形態素1つ（小文字にしたもの）
#[derive(Debug, PartialEq)]
struct Word {
    surface: String,
    base: String,
}

fn segmenter() -> Option<&'static Segmenter> {
    static SEGMENTER: OnceLock<Option<Segmenter>> = OnceLock::new();
    SEGMENTER
        .get_or_init(|| {
            load_dictionary("embedded://ipadic")
                .ok()
                .map(|dict| Segmenter::new(Mode::Normal, dict, None))
        })
        .as_ref()
}

/// 検索に使う形態素。助詞・助動詞・記号・フィラーと空白は落とす
fn analyze(text: &str) -> Vec<Word> {
    let Some(segmenter) = segmenter() else {
        return Vec::new();
    };
    let Ok(tokens) = segmenter.segment(Cow::Borrowed(text)) else {
        return Vec::new();
    };
    let mut words = Vec::new();
    for mut token in tokens {
        let surface = token.surface.trim().to_lowercase();
        if surface.is_empty() {
            continue;
        }
        let details = token.details();
        if matches!(
            details.first().copied(),
            Some("助詞" | "助動詞" | "記号" | "フィラー")
        ) {
            continue;
        }
        let base = details
            .get(6)
            .filter(|b| !b.is_empty() && **b != "*")
            .map(|b| b.to_lowercase())
            .unwrap_or_else(|| surface.clone());
        words.push(Word { surface, base });
    }
    words
}

/// 2文字ずつの組（文字位置つき）。空白をまたぐ組は作らない
fn bigrams(text: &str) -> Vec<(usize, String)> {
    let chars: Vec<char> = text.to_lowercase().chars().collect();
    chars
        .windows(2)
        .enumerate()
        .filter(|(_, pair)| !pair.iter().any(|c| c.is_whitespace()))
        .map(|(i, pair)| (i, pair.iter().collect()))
        .collect()
}

/// tantivy に渡すトークン列（先に全部作っておく）
struct Tokens {
    tokens: Vec<Token>,
    next: usize,
}

impl Tokens {
    fn new(texts: impl IntoIterator<Item = (usize, String)>) -> Self {
        let tokens = texts
            .into_iter()
            .map(|(position, text)| Token {
                position,
                text,
                ..Token::default()
            })
            .collect();
        Tokens { tokens, next: 0 }
    }
}

impl TokenStream for Tokens {
    fn advance(&mut self) -> bool {
        self.next += 1;
        self.next <= self.tokens.len()
    }
    fn token(&self) -> &Token {
        &self.tokens[self.next - 1]
    }
    fn token_mut(&mut self) -> &mut Token {
        &mut self.tokens[self.next - 1]
    }
}

/// 形態素の表層形と基本形（同じ位置に両方置く）。「取り下げ」（名詞）と「取り下げたい」（動詞）のように
/// 品詞で基本形が分かれる語も、表層形どうしで当たる
#[derive(Clone)]
struct JaTokenizer;

impl Tokenizer for JaTokenizer {
    type TokenStream<'a> = Tokens;
    fn token_stream<'a>(&'a mut self, text: &'a str) -> Tokens {
        Tokens::new(
            analyze(text)
                .into_iter()
                .enumerate()
                .flat_map(|(i, w)| forms(w).into_iter().map(move |form| (i, form))),
        )
    }
}

/// 表層形と基本形（同じなら1つ）
fn forms(word: Word) -> Vec<String> {
    if word.surface == word.base {
        vec![word.surface]
    } else {
        vec![word.surface, word.base]
    }
}

/// 2文字ずつ（部分一致用）
#[derive(Clone)]
struct BigramTokenizer;

impl Tokenizer for BigramTokenizer {
    type TokenStream<'a> = Tokens;
    fn token_stream<'a>(&'a mut self, text: &'a str) -> Tokens {
        Tokens::new(bigrams(text))
    }
}

// ---- 索引 ----

#[derive(Clone, Copy)]
struct Fields {
    rel: Field,
    date: Field,
    domain: Field,
    projects: Field,
    tags: Field,
    kind: Field,
    status: Field,
    title_ja: Field,
    body_ja: Field,
    title_bi: Field,
    body_bi: Field,
}

fn schema() -> (Schema, Fields) {
    let mut b = Schema::builder();
    let text = |tokenizer: &str, record: IndexRecordOption| {
        TextOptions::default().set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer(tokenizer)
                .set_index_option(record),
        )
    };
    let ja = text("ika_ja", IndexRecordOption::WithFreqs);
    let bi = text("ika_bigram", IndexRecordOption::WithFreqsAndPositions);
    let fields = Fields {
        rel: b.add_text_field("rel", STRING | STORED),
        date: b.add_text_field("date", STORED),
        domain: b.add_text_field("domain", STRING),
        projects: b.add_text_field("projects", STRING),
        tags: b.add_text_field("tags", STRING),
        kind: b.add_text_field("type", STRING),
        status: b.add_text_field("status", STRING),
        title_ja: b.add_text_field("title_ja", ja.clone()),
        body_ja: b.add_text_field("body_ja", ja),
        title_bi: b.add_text_field("title_bi", bi.clone()),
        body_bi: b.add_text_field("body_bi", bi),
    };
    (b.build(), fields)
}

/// 索引の更新で入れ直したファイルの数
#[derive(Default)]
struct Stats {
    added: usize,
    updated: usize,
    removed: usize,
}

/// 索引に入れたファイルの更新時刻（ナノ秒）と大きさ。変わったものだけ入れ直すのに使う
#[derive(Serialize, Deserialize, Default)]
struct Manifest {
    version: u32,
    files: BTreeMap<String, (u128, u64)>,
}

struct Fts {
    index: Index,
    fields: Fields,
}

impl Fts {
    /// 索引を開き、kb/ の変更を反映する。作りが古い・壊れているとき（と `rebuild`）は作り直す。
    /// 別の ika が書き込み中で反映できなかったときは、更新前の索引と None を返す
    fn open(root: &Path, rebuild: bool) -> Result<(Fts, Option<Stats>)> {
        // 索引は Markdown から作り直せるので、KB の git に入れない置き場に作る
        let dir = crate::root::cache_dir(root)?.join("fts");
        let manifest_path = dir.join("manifest.json");
        let index_dir = dir.join("index");

        let mut manifest: Manifest = fs::read(&manifest_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let (schema, fields) = schema();
        let reuse = !rebuild && manifest.version == VERSION && index_dir.is_dir();
        let index = match reuse.then(|| Index::open_in_dir(&index_dir)).transpose() {
            Ok(Some(index)) if index.schema() == schema => index,
            _ => {
                if index_dir.exists() {
                    fs::remove_dir_all(&index_dir)?;
                }
                fs::create_dir_all(&index_dir)?;
                manifest = Manifest {
                    version: VERSION,
                    files: BTreeMap::new(),
                };
                Index::create_in_dir(&index_dir, schema)?
            }
        };
        index.tokenizers().register("ika_ja", JaTokenizer);
        index.tokenizers().register("ika_bigram", BigramTokenizer);
        let fts = Fts { index, fields };
        let stats = fts.update(root, &mut manifest, &manifest_path)?;
        Ok((fts, stats))
    }

    fn update(
        &self,
        root: &Path,
        manifest: &mut Manifest,
        manifest_path: &Path,
    ) -> Result<Option<Stats>> {
        let mut paths = Vec::new();
        check::collect_markdown(&root.join("kb"), &mut paths)?;
        let mut current = BTreeMap::new();
        for path in paths {
            let meta = fs::metadata(&path)?;
            let mtime = meta
                .modified()?
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default();
            current.insert(rel_string(root, &path), (mtime, meta.len()));
        }
        let changed: Vec<&String> = current
            .iter()
            .filter(|(rel, stamp)| manifest.files.get(*rel) != Some(stamp))
            .map(|(rel, _)| rel)
            .collect();
        let removed: Vec<&String> = manifest
            .files
            .keys()
            .filter(|rel| !current.contains_key(*rel))
            .collect();
        if changed.is_empty() && removed.is_empty() {
            return Ok(Some(Stats::default()));
        }

        let Some(mut writer) = self.writer()? else {
            return Ok(None);
        };
        let added = changed
            .iter()
            .filter(|rel| !manifest.files.contains_key(**rel))
            .count();
        let stats = Stats {
            added,
            updated: changed.len() - added,
            removed: removed.len(),
        };
        let f = self.fields;
        for rel in removed.iter().chain(&changed) {
            writer.delete_term(Term::from_field_text(f.rel, rel));
        }
        for rel in &changed {
            let text = fs::read_to_string(root.join(String::as_str(rel)))
                .with_context(|| format!("{rel} を読めません"))?;
            writer.add_document(self.document(rel, &text))?;
        }
        writer.commit()?;

        manifest.files = current;
        let tmp = manifest_path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec(&*manifest)?)?;
        fs::rename(&tmp, manifest_path)?;
        Ok(Some(stats))
    }

    /// 書き込みのロックを取る。別の ika が持っていれば少し待ち、取れなければ None
    fn writer(&self) -> Result<Option<IndexWriter>> {
        for _ in 0..20 {
            match self.index.writer_with_num_threads(1, 50_000_000) {
                Ok(writer) => return Ok(Some(writer)),
                Err(TantivyError::LockFailure(LockError::LockBusy, _)) => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(err) => return Err(err.into()),
            }
        }
        Ok(None)
    }

    fn document(&self, rel: &str, text: &str) -> TantivyDocument {
        let f = self.fields;
        let fields = frontmatter::parse(text).ok().flatten().unwrap_or_default();
        let field = |key: &str| fields.get(key).cloned().unwrap_or_default();
        let list = |key: &str| {
            fields
                .get(key)
                .and_then(|v| frontmatter::list(v).ok())
                .unwrap_or_default()
        };
        let title = Some(field("title"))
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| rel.rsplit('/').next().unwrap_or(rel).to_string());

        let mut doc = TantivyDocument::default();
        doc.add_text(f.rel, rel);
        doc.add_text(f.date, field("date"));
        if let Some(domain) = rel.split('/').nth(1) {
            doc.add_text(f.domain, domain);
        }
        for project in list("projects") {
            doc.add_text(f.projects, project);
        }
        for tag in list("tags") {
            doc.add_text(f.tags, tag);
        }
        doc.add_text(f.kind, field("type"));
        doc.add_text(f.status, field("status"));
        doc.add_text(f.title_ja, &title);
        doc.add_text(f.body_ja, frontmatter::body(text));
        doc.add_text(f.title_bi, &title);
        // 部分一致は `kb search` と同じくフロントマターも含めた全文で
        doc.add_text(f.body_bi, text);
        doc
    }

    /// 該当件数と、関連度順（検索語がなければ新しい日付順）に並べたファイルのパス
    fn search(&self, args: &Args) -> Result<(usize, Vec<String>)> {
        let f = self.fields;
        let mut clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();
        let filters = [
            (f.domain, &args.domain),
            (f.projects, &args.project),
            (f.tags, &args.tag),
            (f.kind, &args.kind),
            (f.status, &args.status),
        ];
        for (field, value) in filters {
            if let Some(value) = value {
                let term = TermQuery::new(
                    Term::from_field_text(field, value),
                    IndexRecordOption::Basic,
                );
                clauses.push((
                    Occur::Must,
                    Box::new(ConstScoreQuery::new(Box::new(term), 0.0)),
                ));
            }
        }
        let terms: Vec<Box<dyn Query>> = args.terms.iter().map(|t| self.term_query(t)).collect();
        let has_terms = !terms.is_empty();
        if has_terms {
            let occur = if args.any { Occur::Should } else { Occur::Must };
            let words = BooleanQuery::new(terms.into_iter().map(|q| (occur, q)).collect());
            clauses.push((Occur::Must, Box::new(words)));
        }
        let query: Box<dyn Query> = if clauses.is_empty() {
            Box::new(AllQuery)
        } else {
            Box::new(BooleanQuery::new(clauses))
        };

        let reader = self
            .index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;
        let searcher = reader.searcher();
        let all = (searcher.num_docs() as usize).max(1);
        let limit = match args.limit {
            Some(n) => n.max(1),
            None if has_terms => DEFAULT_LIMIT,
            None => all,
        };
        // 検索語がなければ日付で並べ直すので、全件を取ってから絞る
        let fetch = if has_terms { limit } else { all };
        let (top, total) = searcher.search(
            &query,
            &(TopDocs::with_limit(fetch).order_by_score(), Count),
        )?;
        let mut hits = Vec::new();
        for (_, address) in top {
            let doc: TantivyDocument = searcher.doc(address)?;
            let text = |field| {
                doc.get_first(field)
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string()
            };
            hits.push((text(f.date), text(f.rel)));
        }
        if !has_terms {
            // 検索語がないときは関連度がないので、新しい日付順
            hits.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
            hits.truncate(limit);
        }
        Ok((total, hits.into_iter().map(|(_, rel)| rel).collect()))
    }

    /// 検索語1つ分。形態素がすべて含まれるか、語そのものが部分一致すれば当たる。題名は重く数える
    fn term_query(&self, term: &str) -> Box<dyn Query> {
        let f = self.fields;
        let lower = term.to_lowercase();
        let mut alternatives: Vec<(Occur, Box<dyn Query>)> = Vec::new();

        // 形態素ごとに、表層形か基本形のどちらかが題名か本文にあればよい
        let words: BTreeSet<Vec<String>> = analyze(&lower).into_iter().map(forms).collect();
        if !words.is_empty() {
            let all = words
                .iter()
                .map(|forms| {
                    let either: Vec<(Occur, Box<dyn Query>)> = forms
                        .iter()
                        .flat_map(|form| {
                            [
                                (Occur::Should, boosted(text_term(f.title_ja, form), 2.0)),
                                (Occur::Should, text_term(f.body_ja, form)),
                            ]
                        })
                        .collect();
                    (
                        Occur::Must,
                        Box::new(BooleanQuery::new(either)) as Box<dyn Query>,
                    )
                })
                .collect();
            alternatives.push((Occur::Should, Box::new(BooleanQuery::new(all))));
        }
        // 空白で区切られた部分ごとに、2文字ずつの並びが連続して現れるか（＝部分一致）。1文字の部分は見ない
        let parts: Vec<Box<dyn Query>> = lower
            .split_whitespace()
            .filter_map(|part| {
                let grams = bigrams(part);
                if grams.is_empty() {
                    return None;
                }
                let either: Vec<(Occur, Box<dyn Query>)> = vec![
                    (Occur::Should, boosted(phrase(f.title_bi, &grams), 2.0)),
                    (Occur::Should, phrase(f.body_bi, &grams)),
                ];
                Some(Box::new(BooleanQuery::new(either)) as Box<dyn Query>)
            })
            .collect();
        if !parts.is_empty() {
            let all = parts.into_iter().map(|q| (Occur::Must, q)).collect();
            alternatives.push((Occur::Should, Box::new(BooleanQuery::new(all))));
        }
        if alternatives.is_empty() {
            // 1文字の助詞だけなど、どちらでも引けない語は何にも当たらない
            return Box::new(EmptyQuery);
        }
        Box::new(BooleanQuery::new(alternatives))
    }
}

fn text_term(field: Field, text: &str) -> Box<dyn Query> {
    Box::new(TermQuery::new(
        Term::from_field_text(field, text),
        IndexRecordOption::WithFreqs,
    ))
}

fn boosted(query: Box<dyn Query>, boost: f32) -> Box<dyn Query> {
    Box::new(BoostQuery::new(query, boost))
}

fn phrase(field: Field, grams: &[(usize, String)]) -> Box<dyn Query> {
    if let [(_, gram)] = grams {
        return text_term(field, gram);
    }
    let start = grams[0].0;
    let terms = grams
        .iter()
        .map(|(pos, gram)| (pos - start, Term::from_field_text(field, gram)))
        .collect();
    Box::new(PhraseQuery::new_with_offset(terms))
}

/// IKA_HOME からの相対パス（区切りは `/`）
fn rel_string(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.iter()
        .map(|p| p.to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyze_drops_particles_and_uses_base_forms() {
        let bases: Vec<String> = analyze("申請を取り下げたい")
            .into_iter()
            .map(|w| w.base)
            .collect();
        assert_eq!(bases, ["申請", "取り下げる"]);
        let words = analyze("DynamoDB の Next.js");
        assert!(words.iter().all(|w| w.surface == w.surface.to_lowercase()));
        assert!(words.iter().any(|w| w.surface == "dynamodb"), "{words:?}");
    }

    #[test]
    fn bigrams_skip_pairs_across_whitespace() {
        assert_eq!(bigrams("Ab c"), [(0, "ab".to_string())],);
        assert_eq!(
            bigrams("申請書"),
            [(0, "申請".to_string()), (1, "請書".to_string())]
        );
        assert!(bigrams("a").is_empty());
    }
}
