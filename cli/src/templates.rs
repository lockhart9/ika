//! ノートのひな形。正本は リポジトリの templates/ で、ビルド時にバイナリへ埋め込む
//! （KB は ika のリポジトリとは別の場所にあるので、実行時にリポジトリを読まなくて済むように）

pub fn get(name: &str) -> Option<&'static str> {
    Some(match name {
        "note" => include_str!("../../templates/note.md"),
        "insight" => include_str!("../../templates/insight.md"),
        "decision" => include_str!("../../templates/decision.md"),
        "spec" => include_str!("../../templates/spec.md"),
        "project" => include_str!("../../templates/project.md"),
        _ => return None,
    })
}
