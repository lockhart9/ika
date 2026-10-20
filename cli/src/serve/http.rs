//! 読み取り専用のローカルサーバーに必要なだけの HTTP/1.1。
//! GET と HEAD だけを受け付け、1接続1リクエストで閉じる。

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

const MAX_HEADER_BYTES: usize = 16 * 1024;
const READ_TIMEOUT: Duration = Duration::from_secs(5);

pub struct Request {
    pub method: String,
    /// パーセントデコード済みのパス（クエリを除く）
    pub path: String,
    /// パーセントデコード済みのクエリ（`+` は空白）
    pub query: Vec<(String, String)>,
    pub host: Option<String>,
}

impl Request {
    pub fn param(&self, key: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(k, v)| k == key && !v.is_empty())
            .map(|(_, v)| v.as_str())
    }
}

pub struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

impl Response {
    pub fn html(status: u16, body: String) -> Self {
        Self {
            status,
            content_type: "text/html; charset=utf-8",
            body: body.into_bytes(),
        }
    }

    pub fn bytes(content_type: &'static str, body: Vec<u8>) -> Self {
        Self {
            status: 200,
            content_type,
            body,
        }
    }
}

/// リクエストを読む。読めなければ返すべきステータスを `Err` にする
pub fn read_request(stream: &mut TcpStream) -> Result<Request, u16> {
    stream
        .set_read_timeout(Some(READ_TIMEOUT))
        .map_err(|_| 500u16)?;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 2048];
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
        if buf.len() > MAX_HEADER_BYTES {
            return Err(431);
        }
        match stream.read(&mut chunk) {
            Ok(0) => return Err(400),
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(_) => return Err(408),
        }
    }
    let head = String::from_utf8_lossy(&buf);
    let mut lines = head.split("\r\n");
    let mut first = lines.next().unwrap_or("").split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (first.next(), first.next(), first.next(), first.next())
    else {
        return Err(400);
    };
    if !version.starts_with("HTTP/1.") || !target.starts_with('/') {
        return Err(400);
    }
    let host = lines
        .take_while(|l| !l.is_empty())
        .find_map(|l| {
            l.split_once(':')
                .filter(|(k, _)| k.eq_ignore_ascii_case("host"))
        })
        .map(|(_, v)| v.trim().to_string());

    let (raw_path, raw_query) = target.split_once('?').unwrap_or((target, ""));
    let path = percent_decode(raw_path, false).ok_or(400u16)?;
    let query = raw_query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            Some((percent_decode(k, true)?, percent_decode(v, true)?))
        })
        .collect::<Option<Vec<_>>>()
        .ok_or(400u16)?;
    Ok(Request {
        method: method.to_string(),
        path,
        query,
        host,
    })
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

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        431 => "Request Header Fields Too Large",
        _ => "Internal Server Error",
    }
}

pub fn write_response(
    stream: &mut TcpStream,
    response: &Response,
    head_only: bool,
) -> std::io::Result<()> {
    let mut head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n",
        response.status,
        reason(response.status),
        response.content_type,
        response.body.len()
    );
    // 画像は自分のオリジンと https、フォントは Google Fonts だけ。スクリプトは一切実行しない。ほかのページへ情報を渡さない
    for (key, value) in [
        (
            "Content-Security-Policy",
            "default-src 'none'; style-src 'unsafe-inline' https://fonts.googleapis.com; \
             font-src https://fonts.gstatic.com; img-src 'self' https: data:; \
             form-action 'self'; base-uri 'none'; frame-ancestors 'none'",
        ),
        ("X-Content-Type-Options", "nosniff"),
        ("Referrer-Policy", "no-referrer"),
        ("Cache-Control", "no-store"),
        ("Connection", "close"),
    ] {
        head.push_str(&format!("{key}: {value}\r\n"));
    }
    if response.status == 405 {
        head.push_str("Allow: GET, HEAD\r\n");
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    if !head_only {
        stream.write_all(&response.body)?;
    }
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_decode_handles_utf8_and_rejects_garbage() {
        assert_eq!(percent_decode("%E6%97%A5%E6%9C%AC", false).unwrap(), "日本");
        assert_eq!(percent_decode("a+b", true).unwrap(), "a b");
        assert_eq!(percent_decode("a+b", false).unwrap(), "a+b");
        assert!(percent_decode("%zz", false).is_none());
        assert!(percent_decode("%", false).is_none());
        assert!(percent_decode("%00", false).is_none());
        assert!(percent_decode("%FF", false).is_none(), "UTF-8 でない");
    }

    #[test]
    fn percent_encode_round_trips() {
        let path = "kb/tech/notes/2026/日本語 a&b.md";
        let encoded = percent_encode(path);
        assert!(!encoded.contains(' ') && !encoded.contains('&'));
        assert_eq!(percent_decode(&encoded, false).unwrap(), path);
    }
}
