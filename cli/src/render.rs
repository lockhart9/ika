//! Markdown を HTML にする（`ika serve` と `ika export html` で共通）。
//! 本文中の生の HTML は実行されないよう文字として表示し、図の記法（ika-diagram）とコールアウト（`> [!NOTE]`）を展開する。
//! リンクと画像の行き先は呼び出し側が決める（serve は `/kb/...`、export は相対リンク・data URI）

use std::ops::Range;

use pulldown_cmark::{
    CodeBlockKind, CowStr, Event, HeadingLevel, LinkType, Options, Parser, Tag, TagEnd,
    TextMergeWithOffset, html,
};

/// コールアウト（GitHub の `> [!NOTE]` など）のスタイル。見出しの文言は CSS で付ける
pub const CALLOUT_CSS: &str = r#"
blockquote[class^="markdown-alert-"] { --c:var(--accent); --t:var(--accent-tint); margin:1.8rem 0; padding:.9rem 1.3rem 1rem; color:var(--ink);
  background:var(--t); border:0; border-left:3px solid var(--c); border-radius:0 8px 8px 0; }
blockquote[class^="markdown-alert-"]::before { display:block; font:700 .8rem/1.6 var(--sans); color:var(--c); margin-bottom:.15rem; }
blockquote[class^="markdown-alert-"] > :first-child { margin-top:0; } blockquote[class^="markdown-alert-"] > :last-child { margin-bottom:0; }
blockquote.markdown-alert-note::before { content:"メモ"; }
blockquote.markdown-alert-important::before { content:"重要"; }
blockquote.markdown-alert-tip { --c:var(--moss); --t:var(--moss-tint); } blockquote.markdown-alert-tip::before { content:"ヒント"; }
blockquote.markdown-alert-warning { --c:var(--gold); --t:var(--gold-tint); } blockquote.markdown-alert-warning::before { content:"注意"; }
blockquote.markdown-alert-caution { --c:var(--gold); --t:var(--gold-tint); } blockquote.markdown-alert-caution::before { content:"警告"; }
.ikd-error { margin:1.5rem 0; padding:.8rem 1.1rem; border:1.5px solid var(--gold); border-radius:8px; background:var(--gold-tint); font:.85rem/1.7 var(--sans); }
.ikd-error ul { margin:.3rem 0 .6rem; padding-left:1.2rem; }
.ikd-error pre { margin:0; }
.nolink { text-decoration:underline dotted; text-underline-offset:.22em; }
.kbpath { margin-left:.35em; font:.75em var(--mono); color:var(--ink-3); }
"#;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    Link,
    Image,
}

/// リンク・画像の行き先
pub enum Href {
    /// そのまま `href`・`src` にする
    Url(String),
    /// 画像を、この URL へのリンクにする（外部の画像を読み込まないとき）
    LinkTo(String),
    /// `href` を外して文字だけ残し、この文字（KB のパスなど）を小さく添える
    Text(String),
}

pub struct RenderOptions<'a> {
    /// `safe_url` を通した URL から行き先を決める
    pub link: &'a dyn Fn(LinkKind, &str) -> Href,
    /// 図の記法を展開する（取り込んだ資料では展開しない）
    pub diagrams: bool,
    /// 見出しの id の接頭辞（複数のページを1ファイルに束ねるときに重ならないように）
    pub id_prefix: &'a str,
    /// 見出しを何段下げるか（束ねたときに文書の題を h2 にするため）
    pub heading_shift: u8,
}

pub struct Rendered {
    pub html: String,
    pub headings: Vec<Heading>,
    pub errors: Vec<DiagramError>,
}

pub struct Heading {
    /// ずらす前の段（1〜6）
    pub level: u8,
    pub id: String,
    pub text: String,
}

/// 図の記法の誤り。`line` は渡した本文の中の行番号（1 始まり）
pub struct DiagramError {
    pub line: usize,
    pub message: String,
}

/// 開いている図のブロック
struct OpenDiagram {
    lang: String,
    caption: String,
    /// 中身の1行目の、本文の中の行番号
    first_line: usize,
    src: String,
}

/// 閉じるときに出すもの
enum Close {
    Tag,
    Html(String),
}

pub fn markdown(text: &str, opts: &RenderOptions) -> Rendered {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_GFM;
    let parser = Parser::new_ext(text, options).into_offset_iter();
    let mut events: Vec<Event> = Vec::new();
    let mut headings = Vec::new();
    let mut errors = Vec::new();
    let mut ids = Vec::<String>::new();
    let mut figures = 0;
    let mut in_link = false;
    let mut in_code = false;
    let mut diagram: Option<OpenDiagram> = None;
    // 開いたリンク・画像の閉じ方
    let mut closes: Vec<Close> = Vec::new();
    // 開いている見出し（events の中の位置・段・文字）
    let mut heading: Option<(usize, u8, String)> = None;

    for (event, range) in TextMergeWithOffset::new(parser) {
        if let Some(open) = diagram.as_mut() {
            match event {
                Event::Text(t) => open.src.push_str(&t),
                Event::End(TagEnd::CodeBlock) => {
                    let open = diagram.take().expect("open");
                    // 番号は説明の付いた図だけで数える
                    if !open.caption.is_empty() {
                        figures += 1;
                    }
                    events.push(Event::Html(
                        diagram_html(&open, figures, &mut errors).into(),
                    ));
                }
                _ => {}
            }
            continue;
        }
        if let Some((_, _, text)) = heading.as_mut()
            && let Event::Text(t) | Event::Code(t) = &event
        {
            text.push_str(t);
        }
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info)))
                if opts.diagrams && is_diagram(&info) =>
            {
                let (lang, caption) = info
                    .split_once(char::is_whitespace)
                    .unwrap_or((info.as_ref(), ""));
                diagram = Some(OpenDiagram {
                    lang: lang.to_string(),
                    caption: caption.trim().to_string(),
                    first_line: line_of(text, &range) + 1,
                    src: String::new(),
                });
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                in_code = true;
                events.push(Event::Start(Tag::CodeBlock(kind)));
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code = false;
                events.push(Event::End(TagEnd::CodeBlock));
            }
            Event::Start(Tag::Heading {
                level,
                classes,
                attrs,
                ..
            }) => {
                heading = Some((events.len(), level as u8, String::new()));
                events.push(Event::Start(Tag::Heading {
                    level: shift(level, opts.heading_shift),
                    id: None,
                    classes,
                    attrs,
                }));
            }
            Event::End(TagEnd::Heading(level)) => {
                if let Some((at, original, text)) = heading.take() {
                    let id = unique(&mut ids, &format!("{}{}", opts.id_prefix, slug(&text)));
                    if let Event::Start(Tag::Heading { id: slot, .. }) = &mut events[at] {
                        *slot = Some(id.clone().into());
                    }
                    headings.push(Heading {
                        level: original,
                        id,
                        text: text.trim().to_string(),
                    });
                }
                events.push(Event::End(TagEnd::Heading(shift(
                    level,
                    opts.heading_shift,
                ))));
            }
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                in_link = true;
                match (opts.link)(LinkKind::Link, &safe_url(dest_url)) {
                    Href::Url(url) | Href::LinkTo(url) => {
                        closes.push(Close::Tag);
                        events.push(Event::Start(Tag::Link {
                            link_type,
                            dest_url: url.into(),
                            title,
                            id,
                        }));
                    }
                    Href::Text(note) => {
                        closes.push(Close::Html(format!(
                            "</span><span class=\"kbpath\">{}</span>",
                            esc(&note)
                        )));
                        events.push(Event::Html("<span class=\"nolink\">".into()));
                    }
                }
            }
            Event::End(TagEnd::Link) => {
                in_link = false;
                events.push(close(closes.pop(), Event::End(TagEnd::Link)));
            }
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            }) => match (opts.link)(LinkKind::Image, &safe_url(dest_url)) {
                Href::Url(url) => {
                    closes.push(Close::Tag);
                    events.push(Event::Start(Tag::Image {
                        link_type,
                        dest_url: url.into(),
                        title,
                        id,
                    }));
                }
                Href::LinkTo(url) => {
                    closes.push(Close::Html("</a>".into()));
                    events.push(Event::Html(
                        format!("<a href=\"{}\">画像: ", esc(&url)).into(),
                    ));
                }
                Href::Text(note) => {
                    closes.push(Close::Html(format!(
                        "</span><span class=\"kbpath\">{}</span>",
                        esc(&note)
                    )));
                    events.push(Event::Html("<span class=\"nolink\">画像: ".into()));
                }
            },
            Event::End(TagEnd::Image) => {
                events.push(close(closes.pop(), Event::End(TagEnd::Image)));
            }
            // 本文中の生の HTML は文字として表示する
            Event::Html(raw) | Event::InlineHtml(raw) => events.push(Event::Text(raw)),
            Event::SoftBreak => events.push(Event::HardBreak),
            Event::Text(t) if !in_link && !in_code => events.extend(autolink(t)),
            other => events.push(other),
        }
    }
    let mut out = String::new();
    html::push_html(&mut out, events.into_iter());
    Rendered {
        html: out,
        headings,
        errors,
    }
}

/// 本文の図の記法だけを確かめる（`ika check` 用。HTML は作らない）
pub fn diagram_errors(text: &str) -> Vec<DiagramError> {
    let mut errors = Vec::new();
    let mut open: Option<OpenDiagram> = None;
    for (event, range) in Parser::new_ext(text, Options::empty()).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) if is_diagram(&info) => {
                let lang = info.split_whitespace().next().unwrap_or_default();
                open = Some(OpenDiagram {
                    lang: lang.to_string(),
                    caption: String::new(),
                    first_line: line_of(text, &range) + 1,
                    src: String::new(),
                });
            }
            Event::Text(t) => {
                if let Some(open) = open.as_mut() {
                    open.src.push_str(&t);
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(open) = open.take() {
                    diagram_html(&open, 0, &mut errors);
                }
            }
            _ => {}
        }
    }
    errors
}

fn close(close: Option<Close>, tag: Event<'static>) -> Event<'static> {
    match close {
        Some(Close::Html(html)) => Event::Html(html.into()),
        _ => tag,
    }
}

fn is_diagram(info: &str) -> bool {
    info.split_whitespace()
        .next()
        .is_some_and(|lang| ika_diagram::LANGS.contains(&lang))
}

/// `range` の始まりの行番号（1 始まり）
fn line_of(text: &str, range: &Range<usize>) -> usize {
    text[..range.start].matches('\n').count() + 1
}

/// 図を描く。誤りがあれば元の記法と理由を見せ、`errors` に足す
fn diagram_html(open: &OpenDiagram, number: usize, errors: &mut Vec<DiagramError>) -> String {
    match ika_diagram::parse(&open.lang, &open.src).expect("図の言語名") {
        Ok(diagram) => ika_diagram::figure(&ika_diagram::render(&diagram), number, &open.caption),
        Err(found) => {
            let mut out = format!(
                "<div class=\"ikd-error\"><b>図の記法の誤り（{}）</b><ul>",
                esc(&open.lang)
            );
            for error in found {
                let line = open.first_line + error.line - 1;
                out.push_str(&format!("<li>{line}行目: {}</li>", esc(&error.message)));
                errors.push(DiagramError {
                    line,
                    message: format!("図（{}）: {}", open.lang, error.message),
                });
            }
            out.push_str(&format!(
                "</ul><pre><code>{}</code></pre></div>",
                esc(&open.src)
            ));
            out
        }
    }
}

fn shift(level: HeadingLevel, by: u8) -> HeadingLevel {
    HeadingLevel::try_from((level as usize + by as usize).min(6)).unwrap_or(HeadingLevel::H6)
}

/// 見出しの id。空白は `-` にし、ASCII の記号を除く（日本語はそのまま）。英字は小文字
pub fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.trim().chars() {
        if c.is_whitespace() {
            if !out.ends_with('-') {
                out.push('-');
            }
        } else if c.is_ascii_punctuation() && c != '-' && c != '_' {
            continue;
        } else {
            out.extend(c.to_lowercase());
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "section".into()
    } else {
        out
    }
}

/// 同じ id が既にあれば `-2`・`-3` … を付ける
fn unique(ids: &mut Vec<String>, id: &str) -> String {
    let mut candidate = id.to_string();
    let mut n = 1;
    while ids.contains(&candidate) {
        n += 1;
        candidate = format!("{id}-{n}");
    }
    ids.push(candidate.clone());
    candidate
}

/// HTML に埋め込む文字列のエスケープ
pub fn esc(text: &str) -> String {
    ika_diagram::esc(text)
}

/// `http(s):`・`mailto:`・スキームなしの相対リンクだけを通す（`javascript:` などは無効にする）
pub fn safe_url(url: CowStr<'_>) -> String {
    let first = url.split(['/', '?', '#']).next().unwrap_or("");
    if let Some((scheme, _)) = first.split_once(':') {
        let scheme = scheme.to_ascii_lowercase();
        if !matches!(scheme.as_str(), "http" | "https" | "mailto") {
            return "#".into();
        }
    }
    url.to_string()
}

pub fn is_web_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

/// 本文中の裸の `http(s)://` URL をリンクにする（CommonMark には自動リンクがない）
fn autolink(text: CowStr<'_>) -> Vec<Event<'_>> {
    let mut events = Vec::new();
    let mut rest: &str = &text;
    let mut plain_from = 0;
    let mut offset = 0;
    while let Some(found) = rest.find("http") {
        let start = offset + found;
        let tail = &text[start..];
        let has_scheme = tail.starts_with("http://") || tail.starts_with("https://");
        let len = if has_scheme { url_len(tail) } else { 0 };
        let min = if tail.starts_with("https://") { 8 } else { 7 };
        if len > min {
            if plain_from < start {
                events.push(Event::Text(text[plain_from..start].to_string().into()));
            }
            let url = &tail[..len];
            events.push(Event::Start(Tag::Link {
                link_type: LinkType::Autolink,
                dest_url: url.to_string().into(),
                title: "".into(),
                id: "".into(),
            }));
            events.push(Event::Text(url.to_string().into()));
            events.push(Event::End(TagEnd::Link));
            plain_from = start + len;
            offset = plain_from;
        } else {
            offset = start + 4;
        }
        rest = &text[offset..];
    }
    if events.is_empty() {
        return vec![Event::Text(text)];
    }
    if plain_from < text.len() {
        events.push(Event::Text(text[plain_from..].to_string().into()));
    }
    events
}

/// `s`（`http` で始まる）の先頭にある URL の長さ。空白・ASCII 以外・`<>"` で終わり、末尾の句読点は含めない
fn url_len(s: &str) -> usize {
    let mut end = s
        .find(|c: char| !c.is_ascii_graphic() || matches!(c, '<' | '>' | '"' | '`'))
        .unwrap_or(s.len());
    loop {
        let url = &s[..end];
        let Some(last) = url.chars().last() else {
            return end;
        };
        let unbalanced_close = |close: char, open: char| {
            last == close && url.matches(close).count() > url.matches(open).count()
        };
        if matches!(last, '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '*')
            || unbalanced_close(')', '(')
            || unbalanced_close(']', '[')
            || unbalanced_close('}', '{')
        {
            end -= 1;
        } else {
            return end;
        }
    }
}

/// `%XX` を戻す。不正な並びや UTF-8 でなければ `None`。NUL は拒否する
pub fn percent_decode(text: &str, plus_is_space: bool) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let hex = text.get(i + 1..i + 3)?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            }
            b'+' if plus_is_space => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    let decoded = String::from_utf8(out).ok()?;
    (!decoded.contains('\0')).then_some(decoded)
}

/// URL のパス・クエリに使えるよう、英数字と `-_.~/` 以外を `%XX` にする
pub fn percent_encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~' | b'/') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same(kind: LinkKind, url: &str) -> Href {
        let _ = kind;
        Href::Url(url.to_string())
    }

    fn render(text: &str) -> Rendered {
        markdown(
            text,
            &RenderOptions {
                link: &same,
                diagrams: true,
                id_prefix: "",
                heading_shift: 0,
            },
        )
    }

    #[test]
    fn expands_diagrams_with_captions_and_numbers() {
        let out = render(
            "前\n\n```seq 呼び出しの流れ\nA -> B: x\n```\n\n```flow\na -> b\n```\n\n```layers 構成\na: b\n```\n\n```rust\nlet a = 1;\n```\n",
        );
        assert!(out.errors.is_empty());
        assert!(
            out.html.contains(
                "<figure class=\"ikd-fig\"><div class=\"ikd-fig-body\"><div class=\"ikd ikd-seq\">"
            ),
            "{}",
            out.html
        );
        assert!(
            out.html
                .contains("<figcaption><b>図1</b>呼び出しの流れ</figcaption>")
        );
        assert!(out.html.contains("<div class=\"ikd ikd-flow\">"));
        // 説明のない図は数えない
        assert!(out.html.contains("<figcaption><b>図2</b>構成</figcaption>"));
        assert!(
            out.html
                .contains("<code class=\"language-rust\">let a = 1;")
        );
    }

    #[test]
    fn reports_diagram_errors_with_lines_of_the_text_and_shows_the_source() {
        let out = render("# t\n\n```seq\nA -> B: ok\nA ->> B\n```\n");
        assert_eq!(out.errors.len(), 1);
        assert_eq!(out.errors[0].line, 5);
        assert!(out.errors[0].message.starts_with("図（seq）: "));
        assert!(
            out.html.contains("<div class=\"ikd-error\">"),
            "{}",
            out.html
        );
        assert!(out.html.contains("5行目: "));
        assert!(out.html.contains("A -&gt;&gt; B"));
    }

    #[test]
    fn leaves_diagram_blocks_as_code_when_disabled() {
        let out = markdown(
            "```seq\nA ->> B\n```\n",
            &RenderOptions {
                link: &same,
                diagrams: false,
                id_prefix: "",
                heading_shift: 0,
            },
        );
        assert!(out.errors.is_empty());
        assert!(out.html.contains("<code class=\"language-seq\">"));
    }

    #[test]
    fn renders_callouts_and_never_raw_html() {
        let out = render("> [!WARNING]\n> 気をつける\n\n<script>alert(1)</script>\n");
        assert!(
            out.html
                .contains("<blockquote class=\"markdown-alert-warning\">"),
            "{}",
            out.html
        );
        assert!(!out.html.contains("<script>"));
        assert!(out.html.contains("&lt;script&gt;"));
    }

    #[test]
    fn gives_headings_unique_ids_and_shifts_levels() {
        let out = markdown(
            "## 目的 と範囲\n\n### `ika` の使い方!\n\n## 目的 と範囲\n",
            &RenderOptions {
                link: &same,
                diagrams: true,
                id_prefix: "d1-",
                heading_shift: 1,
            },
        );
        let ids: Vec<_> = out
            .headings
            .iter()
            .map(|h| (h.level, h.id.as_str()))
            .collect();
        assert_eq!(
            ids,
            [
                (2, "d1-目的-と範囲"),
                (3, "d1-ika-の使い方"),
                (2, "d1-目的-と範囲-2")
            ]
        );
        assert!(
            out.html.contains("<h3 id=\"d1-目的-と範囲\">"),
            "{}",
            out.html
        );
        assert!(out.html.contains("<h4 id=\"d1-ika-の使い方\">"));
    }

    #[test]
    fn turns_text_links_and_images_into_plain_text_with_the_path() {
        let link = |kind: LinkKind, url: &str| match (kind, url) {
            (LinkKind::Image, u) if u.starts_with("https") => Href::LinkTo(u.into()),
            (_, u) if u.ends_with(".md") || u.ends_with(".png") => Href::Text(format!("kb/{u}")),
            (_, u) => Href::Url(u.into()),
        };
        let out = markdown(
            "[a](x.md) [b](https://e.test/) ![c](p.png) ![d](https://e.test/i.png)",
            &RenderOptions {
                link: &link,
                diagrams: true,
                id_prefix: "",
                heading_shift: 0,
            },
        );
        assert!(
            out.html
                .contains("<span class=\"nolink\">a</span><span class=\"kbpath\">kb/x.md</span>"),
            "{}",
            out.html
        );
        assert!(out.html.contains("<a href=\"https://e.test/\">b</a>"));
        assert!(out.html.contains(
            "<span class=\"nolink\">画像: c</span><span class=\"kbpath\">kb/p.png</span>"
        ));
        assert!(
            out.html
                .contains("<a href=\"https://e.test/i.png\">画像: d</a>")
        );
    }

    #[test]
    fn diagram_errors_match_the_rendered_ones() {
        let text = "x\n\n~~~flow 説明\na -> -> b\n~~~\n\n```timeline\n先週: x\n```\n";
        let checked: Vec<_> = diagram_errors(text).into_iter().map(|e| e.line).collect();
        let rendered: Vec<_> = render(text).errors.into_iter().map(|e| e.line).collect();
        assert_eq!(checked, [4, 8]);
        assert_eq!(checked, rendered);
    }

    #[test]
    fn slug_keeps_japanese_and_drops_ascii_punctuation() {
        assert_eq!(slug("  Hello, World!  "), "hello-world");
        assert_eq!(slug("課題・次の打ち手"), "課題・次の打ち手");
        assert_eq!(slug("!!!"), "section");
    }
}
