//! stdout への出力。`ika ... | head` のようにパイプが先に閉じられても、panic せず静かに終わる。
//!
//! Rust は SIGPIPE を無視して起動するので、閉じたパイプへの書き込みは EPIPE のエラーになり、
//! 標準の `println!` はそこで panic する。シグナルの設定を戻すには `unsafe` と libc が要るので、
//! 代わりに同名のマクロで標準のものを隠し、BrokenPipe だけを受けて終わる。
//! `writeln!(out, ..)?` のように `?` で伝わったものは [`is_broken_pipe`] で `main` が受ける。

use std::fmt;
use std::io::{self, Write};

/// シグナルで死んだときのシェルの終了コード（128 + SIGPIPE）に揃える
pub const BROKEN_PIPE_EXIT: u8 = 141;

/// `println!` / `print!` の本体
pub fn print(args: fmt::Arguments<'_>) {
    if let Err(err) = io::stdout().write_fmt(args) {
        if err.kind() == io::ErrorKind::BrokenPipe {
            std::process::exit(i32::from(BROKEN_PIPE_EXIT));
        }
        panic!("failed printing to stdout: {err}");
    }
}

/// エラーの原因をたどって、閉じたパイプへの書き込みかを見る
pub fn is_broken_pipe(err: &anyhow::Error) -> bool {
    err.chain().any(|cause| {
        let io_err = match cause.downcast_ref::<csv::Error>() {
            // csv::Error は Io のとき `source()` を返さないので、ここで開ける
            Some(csv_err) => match csv_err.kind() {
                csv::ErrorKind::Io(io_err) => Some(io_err),
                _ => None,
            },
            None => cause.downcast_ref::<io::Error>(),
        };
        io_err.is_some_and(|io_err| io_err.kind() == io::ErrorKind::BrokenPipe)
    })
}

macro_rules! println {
    () => { $crate::out::print(format_args!("\n")) };
    ($($arg:tt)*) => { $crate::out::print(format_args!("{}\n", format_args!($($arg)*))) };
}

macro_rules! print {
    ($($arg:tt)*) => { $crate::out::print(format_args!($($arg)*)) };
}
