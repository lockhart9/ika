//! PDF の文字を取り出す。ページの区切りに `<!-- page N -->` を入れる（引用するときにページを示せるように）。
//! 見出しや表の構造は PDF に残っていないので、文字の並びだけになる。
//!
//! まず Rust の pdf-extract で読む。panic・エラー・文字が空のときは、PATH に `pdftotext`（poppler）が
//! あれば代わりにそれで読む（日本語フォントの一部は pdf-extract が未対応で panic する）。
//! `pdftotext` の出力は表の桁位置を残すので、ページごとにコードブロックで囲む

use std::any::Any;
use std::io::{ErrorKind, Write};
use std::panic;
use std::process::{Command, Output, Stdio};
use std::thread;

use anyhow::{Result, bail};

use super::convert::Converted;

type Pages = Vec<String>;

/// 外部の `pdftotext` を使うときの実行ファイル名と引数。PDF は標準入力、文字は標準出力
const PDFTOTEXT: &str = "pdftotext";
const PDFTOTEXT_ARGS: [&str; 5] = ["-layout", "-enc", "UTF-8", "-", "-"];

/// `pdftotext` で読めなかった理由
#[derive(Debug)]
enum Unavailable {
    /// PATH に無い
    NotInstalled,
    /// 実行できたが失敗した（理由つき）
    Failed(String),
}

pub fn convert(bytes: &[u8]) -> Result<Converted> {
    convert_with(bytes, extract, |bytes| {
        pdftotext(PDFTOTEXT, &PDFTOTEXT_ARGS, bytes)
    })
}

/// 最初の読み方（`primary`）で読めなければ、`fallback` で読む。どちらも差し替えられる（テスト用）
fn convert_with(
    bytes: &[u8],
    primary: impl FnOnce(&[u8]) -> Result<Pages, String>,
    fallback: impl FnOnce(&[u8]) -> Result<Pages, Unavailable>,
) -> Result<Converted> {
    // 最初の読み方が失敗した理由。文字が空だっただけなら None
    let failure = match primary(bytes) {
        Ok(pages) if has_text(&pages) => {
            return Ok(converted(bytes, sections(&pages, plain), Vec::new()));
        }
        Ok(_) => None,
        Err(reason) => Some(reason),
    };
    match fallback(bytes) {
        Ok(pages) if has_text(&pages) => {
            let why = match &failure {
                Some(reason) => format!("pdf-extract では読めなかった（{reason}）"),
                None => "pdf-extract では文字が取れなかった".to_string(),
            };
            let warning = format!(
                "{why}ため、pdftotext（poppler）で取り出した。ページごとに桁位置を残した文字になる"
            );
            Ok(converted(bytes, sections(&pages, fenced), vec![warning]))
        }
        other => match failure {
            Some(reason) => {
                let detail = match other {
                    Ok(_) => "pdftotext でも文字を取り出せませんでした".to_string(),
                    Err(Unavailable::NotInstalled) => {
                        "pdftotext（poppler）が PATH にあれば、代わりにそれで読みます（`brew install poppler` など）"
                            .to_string()
                    }
                    Err(Unavailable::Failed(message)) => {
                        format!("pdftotext でも読めませんでした: {message}")
                    }
                };
                bail!("PDF を読めません: {reason}\n{detail}")
            }
            None => bail!(
                "PDF から文字を取り出せません（スキャン画像の PDF かもしれません）。\
                 AI に PDF を読ませて Markdown にし、`ika import - --format md --source <原本>` で渡してください"
            ),
        },
    }
}

fn converted(bytes: &[u8], body: String, warnings: Vec<String>) -> Converted {
    Converted {
        body,
        title: title(bytes),
        warnings,
    }
}

fn has_text(pages: &[String]) -> bool {
    pages.iter().any(|page| !page.trim().is_empty())
}

/// ページごとに `<!-- page N -->` を付けてつなぐ。空のページは出さず、番号は保つ
fn sections(pages: &[String], render: fn(&str) -> String) -> String {
    pages
        .iter()
        .enumerate()
        .filter(|(_, page)| !page.trim().is_empty())
        .map(|(i, page)| format!("<!-- page {} -->\n\n{}", i + 1, render(page)))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn plain(page: &str) -> String {
    page.trim().to_string()
}

/// 桁位置を残すため、先頭の空行と行末の空白だけ落として、コードブロックで囲む。
/// 文字の中にバッククォートが連続していれば、それより長い囲みにする
fn fenced(page: &str) -> String {
    let text = page
        .lines()
        .skip_while(|line| line.trim().is_empty())
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");
    let text = text.trim_end();
    let longest = text
        .split(|c| c != '`')
        .map(str::len)
        .max()
        .unwrap_or_default();
    let fence = "`".repeat((longest + 1).max(3));
    format!("{fence}text\n{text}\n{fence}")
}

/// 文書情報（/Info の /Title）のタイトル。無い・読めないときは None
fn title(bytes: &[u8]) -> Option<String> {
    let doc = lopdf::Document::load_mem(bytes).ok()?;
    let info = match doc.trailer.get(b"Info").ok()? {
        lopdf::Object::Reference(id) => doc.get_object(*id).ok()?,
        other => other,
    };
    let lopdf::Object::String(raw, _) = info.as_dict().ok()?.get(b"Title").ok()? else {
        return None;
    };
    let title = decode_text(raw);
    let title = title.trim();
    (!title.is_empty()).then(|| title.to_string())
}

/// PDF のテキスト文字列を読む。UTF-16BE（BOM 付き）・UTF-8（BOM 付き）、それ以外は Latin-1 とみなす
fn decode_text(raw: &[u8]) -> String {
    if let Some(utf16) = raw.strip_prefix(&[0xfe, 0xff]) {
        let units: Vec<u16> = utf16
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_be_bytes(*pair))
            .collect();
        String::from_utf16_lossy(&units)
    } else if let Some(utf8) = raw.strip_prefix(&[0xef, 0xbb, 0xbf]) {
        String::from_utf8_lossy(utf8).into_owned()
    } else {
        raw.iter().map(|&b| b as char).collect()
    }
}

/// pdf-extract は壊れた・特殊な PDF で panic することがあるので、エラーとして受け止める。
/// 失敗した理由（panic のメッセージを含む）を文字列で返す
fn extract(bytes: &[u8]) -> Result<Pages, String> {
    let hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let result = panic::catch_unwind(|| pdf_extract::extract_text_from_mem_by_pages(bytes));
    panic::set_hook(hook);
    match result {
        Ok(Ok(pages)) => Ok(pages),
        Ok(Err(err)) => Err(format!("pdf-extract: {err}")),
        Err(payload) => Err(format!(
            "pdf-extract が異常終了しました: {}",
            panic_message(&*payload)
        )),
    }
}

fn panic_message(payload: &(dyn Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|message| message.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "原因は不明".to_string())
}

/// 外部コマンド（`pdftotext`）に PDF を標準入力で渡し、改ページ文字（`\f`）で区切られた文字をページに分ける。
/// 標準エラーには、非埋め込みフォントの警告が大量に出るので、失敗したときだけ最後の数行を使う
fn pdftotext(program: &str, args: &[&str], bytes: &[u8]) -> Result<Pages, Unavailable> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| match err.kind() {
            ErrorKind::NotFound => Unavailable::NotInstalled,
            _ => Unavailable::Failed(err.to_string()),
        })?;
    let Some(mut stdin) = child.stdin.take() else {
        return Err(Unavailable::Failed("標準入力を開けません".to_string()));
    };
    // 出力が詰まって固まらないよう、書き込みは別のスレッドで行う。途中で閉じられても（BrokenPipe）無視する
    let output = thread::scope(|scope| {
        scope.spawn(move || {
            let _ = stdin.write_all(bytes);
        });
        child.wait_with_output()
    })
    .map_err(|err| Unavailable::Failed(err.to_string()))?;
    if !output.status.success() {
        return Err(Unavailable::Failed(describe_failure(&output)));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut pages: Pages = text.split('\u{c}').map(str::to_string).collect();
    // 最後の改ページ文字のあとの空は、ページではない
    if pages.last().is_some_and(|page| page.trim().is_empty()) {
        pages.pop();
    }
    Ok(pages)
}

fn describe_failure(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let lines: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let tail = &lines[lines.len().saturating_sub(3)..];
    if tail.is_empty() {
        match output.status.code() {
            Some(code) => format!("終了コード {code}"),
            None => "シグナルで終了しました".to_string(),
        }
    } else {
        tail.join(" / ")
    }
}

#[cfg(test)]
mod tests {
    use lopdf::content::{Content, Operation};
    use lopdf::{Document, Object, Stream, dictionary};

    use super::*;

    /// 1ページに1行ずつ文字を置いた PDF。`title` があれば文書情報に入れる（UTF-16BE）
    fn pdf_of(pages: &[&str], title: Option<&str>) -> Vec<u8> {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        let resources_id = doc.add_object(dictionary! {
            "Font" => dictionary! { "F1" => font_id },
        });
        let mut kids = Vec::new();
        for text in pages {
            let content = Content {
                operations: vec![
                    Operation::new("BT", vec![]),
                    Operation::new("Tf", vec!["F1".into(), 24.into()]),
                    Operation::new("Td", vec![100.into(), 600.into()]),
                    Operation::new("Tj", vec![Object::string_literal(*text)]),
                    Operation::new("ET", vec![]),
                ],
            };
            let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
            let page_id = doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => content_id,
            });
            kids.push(page_id.into());
        }
        let count = kids.len() as i64;
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => kids,
                "Count" => count,
                "Resources" => resources_id,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);
        if let Some(title) = title {
            let mut raw = vec![0xfe, 0xff];
            raw.extend(title.encode_utf16().flat_map(u16::to_be_bytes));
            let info_id = doc.add_object(dictionary! {
                "Title" => Object::String(raw, lopdf::StringFormat::Hexadecimal),
            });
            doc.trailer.set("Info", info_id);
        }
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    }

    /// 日本語フォント（フォントを埋め込まない CID フォント。エンコーディングは UniJIS-UTF16-H）で「あ」を置いた PDF
    fn japanese_font_pdf() -> Vec<u8> {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let descendant_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "CIDFontType2",
            "BaseFont" => "MSMincho",
            "CIDSystemInfo" => dictionary! {
                "Registry" => Object::string_literal("Adobe"),
                "Ordering" => Object::string_literal("Japan1"),
                "Supplement" => 2,
            },
        });
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type0",
            "BaseFont" => "MSMincho",
            "Encoding" => "UniJIS-UTF16-H",
            "DescendantFonts" => vec![descendant_id.into()],
        });
        let resources_id = doc.add_object(dictionary! {
            "Font" => dictionary! { "F1" => font_id },
        });
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 24.into()]),
                Operation::new("Td", vec![100.into(), 600.into()]),
                Operation::new(
                    "Tj",
                    vec![Object::String(
                        vec![0x30, 0x42],
                        lopdf::StringFormat::Hexadecimal,
                    )],
                ),
                Operation::new("ET", vec![]),
            ],
        };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
                "Resources" => resources_id,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    }

    /// 最初の読み方が失敗する（panic はせず、理由だけ返す）差し替え
    fn fails(reason: &str) -> impl FnOnce(&[u8]) -> Result<Pages, String> {
        let reason = reason.to_string();
        move |_| Err(reason)
    }

    /// 呼ばれたら失敗する fallback（呼ばれないことを確かめる）
    fn must_not_run(_: &[u8]) -> Result<Pages, Unavailable> {
        panic!("fallback を呼んではいけない")
    }

    fn pages(texts: &[&str]) -> Pages {
        texts.iter().map(|text| text.to_string()).collect()
    }

    fn error_of(result: Result<Converted>) -> String {
        match result {
            Ok(_) => panic!("エラーになるはず"),
            Err(err) => err.to_string(),
        }
    }

    #[test]
    fn extracts_text_with_page_markers() {
        let converted = convert(&pdf_of(&["Pricing plans", "Free tier"], None)).unwrap();
        assert_eq!(converted.title, None);
        assert!(converted.warnings.is_empty());
        assert!(
            converted.body.starts_with("<!-- page 1 -->\n\n"),
            "{}",
            converted.body
        );
        assert!(converted.body.contains("Pricing plans"));
        assert!(converted.body.contains("<!-- page 2 -->\n\nFree tier"));
    }

    #[test]
    fn title_comes_from_the_document_info() {
        let converted = convert(&pdf_of(&["Body"], Some("働き方改革の手引き"))).unwrap();
        assert_eq!(converted.title.as_deref(), Some("働き方改革の手引き"));
        assert_eq!(decode_text(b"Plain"), "Plain");
    }

    #[test]
    fn broken_pdf_is_an_error_not_a_panic() {
        let result = convert_with(b"%PDF-1.5 broken", extract, |_| {
            Err(Unavailable::NotInstalled)
        });
        assert!(result.is_err());
    }

    #[test]
    fn a_panic_message_is_kept_in_the_error() {
        let message = error_of(convert_with(
            b"x",
            |_| {
                panic::catch_unwind(|| panic!("unsupported encoding X"))
                    .map_err(|p| panic_message(&*p))
            },
            |_| Err(Unavailable::NotInstalled),
        ));
        assert!(message.contains("unsupported encoding X"), "{message}");
        // String の panic（フォーマット付き）も読める
        let name = "Y";
        let payload = panic::catch_unwind(|| panic!("unsupported encoding {name}")).unwrap_err();
        assert_eq!(panic_message(&*payload), "unsupported encoding Y");
    }

    #[test]
    fn a_pdf_with_an_unsupported_font_encoding_is_an_error_with_the_reason() {
        let reason = extract(&japanese_font_pdf()).unwrap_err();
        assert!(reason.contains("UniJIS-UTF16-H"), "{reason}");
    }

    #[cfg(unix)]
    #[test]
    fn falls_back_to_pdftotext_and_splits_pages_at_form_feeds() {
        let converted = convert_with(
            &pdf_of(&["x"], Some("働き方改革の手引き")),
            fails("pdf-extract が異常終了しました: unsupported encoding X"),
            |bytes| {
                pdftotext(
                    "sh",
                    &["-c", "cat >/dev/null; printf 'a  1\\fb  2\\f'"],
                    bytes,
                )
            },
        )
        .unwrap();
        assert_eq!(
            converted.body,
            "<!-- page 1 -->\n\n```text\na  1\n```\n\n<!-- page 2 -->\n\n```text\nb  2\n```"
        );
        assert_eq!(converted.warnings.len(), 1);
        assert!(converted.warnings[0].contains("pdftotext"));
        assert!(converted.warnings[0].contains("unsupported encoding X"));
        // 文書情報のタイトルは fallback でも取れる
        assert_eq!(converted.title.as_deref(), Some("働き方改革の手引き"));
    }

    #[test]
    fn empty_pages_are_skipped_but_numbering_is_kept() {
        let converted =
            convert_with(b"x", fails("no"), |_| Ok(pages(&["a", "", "c", ""]))).unwrap();
        assert!(converted.body.contains("<!-- page 1 -->"));
        assert!(!converted.body.contains("<!-- page 2 -->"));
        assert!(converted.body.contains("<!-- page 3 -->"));
        assert!(!converted.body.contains("<!-- page 4 -->"));
    }

    #[cfg(unix)]
    #[test]
    fn a_trailing_form_feed_does_not_make_an_extra_page() {
        let result =
            pdftotext("sh", &["-c", "cat >/dev/null; printf 'a\\f\\fc\\f'"], b"x").unwrap();
        assert_eq!(result, pages(&["a", "", "c"]));
    }

    #[test]
    fn a_missing_pdftotext_is_reported_with_the_first_reason() {
        let message = error_of(convert_with(
            b"x",
            fails("pdf-extract が異常終了しました: unsupported encoding X"),
            |bytes| pdftotext("ika-no-such-pdftotext", &[], bytes),
        ));
        assert!(message.contains("unsupported encoding X"), "{message}");
        assert!(message.contains("pdftotext"), "{message}");
        assert!(message.contains("poppler"), "{message}");
    }

    #[cfg(unix)]
    #[test]
    fn a_failing_pdftotext_is_reported_with_the_first_reason() {
        let message = error_of(convert_with(b"x", fails("pdf-extract: broken"), |bytes| {
            pdftotext(
                "sh",
                &[
                    "-c",
                    "cat >/dev/null; echo 'Syntax Error: oops' >&2; exit 1",
                ],
                bytes,
            )
        }));
        assert!(message.contains("pdf-extract: broken"), "{message}");
        assert!(
            message.contains("pdftotext でも読めませんでした"),
            "{message}"
        );
        assert!(message.contains("Syntax Error: oops"), "{message}");
        // 標準エラーが無いときは終了コード
        let reason = match pdftotext("sh", &["-c", "cat >/dev/null; exit 3"], b"x") {
            Err(Unavailable::Failed(reason)) => reason,
            _ => panic!("失敗するはず"),
        };
        assert_eq!(reason, "終了コード 3");
    }

    #[test]
    fn the_first_reader_succeeding_never_runs_the_fallback() {
        let converted = convert_with(b"x", |_| Ok(pages(&["Hello"])), must_not_run).unwrap();
        assert_eq!(converted.body, "<!-- page 1 -->\n\nHello");
        assert!(converted.warnings.is_empty());
    }

    #[test]
    fn empty_text_also_tries_the_fallback() {
        let converted = convert_with(
            b"x",
            |_| Ok(pages(&["", "  \n"])),
            |_| Ok(pages(&["from pdftotext"])),
        )
        .unwrap();
        assert!(converted.body.contains("from pdftotext"));
        assert!(converted.warnings[0].contains("文字が取れなかった"));
    }

    #[test]
    fn empty_text_from_both_is_the_scanned_pdf_error() {
        for fallback in [
            Ok(pages(&[""])),
            Err(Unavailable::NotInstalled),
            Err(Unavailable::Failed("x".to_string())),
        ] {
            let message = error_of(convert_with(b"x", |_| Ok(pages(&[""])), move |_| fallback));
            assert!(message.contains("スキャン画像"), "{message}");
        }
    }

    #[test]
    fn a_fallback_with_nothing_after_a_failure_keeps_the_first_reason() {
        let message = error_of(convert_with(b"x", fails("pdf-extract: broken"), |_| {
            Ok(pages(&[""]))
        }));
        assert!(message.contains("pdf-extract: broken"), "{message}");
        assert!(message.contains("文字を取り出せません"), "{message}");
    }

    #[test]
    fn a_fence_is_longer_than_any_run_of_backticks_in_the_text() {
        assert_eq!(fenced("\n\n  a  b  \nc  \n\n"), "```text\n  a  b\nc\n```");
        assert_eq!(fenced("x ``` y"), "````text\nx ``` y\n````");
        assert_eq!(fenced("`` a"), "```text\n`` a\n```");
    }
}
