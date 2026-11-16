//! プロダクトの README の節（「現状」「課題・次の打ち手」）を、箇条書きの単位で扱う

use std::collections::HashSet;

/// 「現状」の節の名前。日付つきの行（直近の動き）が足されていく
pub(crate) const STATUS: &str = "現状";
/// 「課題・次の打ち手」の節の名前
pub(crate) const BACKLOG: &str = "課題・次の打ち手";
/// 要点に出す「現状」の日付つきの行の件数（日付の新しい順）
pub(crate) const STATUS_KEEP: usize = 5;
/// 要点に出す「課題・次の打ち手」の件数（先頭から）
const BACKLOG_KEEP: usize = 20;
/// 要点に出す箇条書き1件の字数
const ITEM_CHARS: usize = 160;

/// 節の中の1かたまり
pub(crate) enum Block<'a> {
    /// 一番上の階層の箇条書き1件
    Item(Item<'a>),
    /// それ以外の行（見出し・段落・空行・コードブロック）
    Line(&'a str),
}

pub(crate) struct Item<'a> {
    /// 行頭が `- YYYY-MM-DD`（太字の `- **YYYY-MM-DD` も）のときの日付
    pub date: Option<&'a str>,
    /// 箇条書きの行（改行つき）
    pub head: &'a str,
    /// すぐ下に続く、字下げした行（改行つき）
    pub children: Vec<&'a str>,
}

impl Item<'_> {
    /// 子の行を含む全文
    fn text(&self) -> String {
        format!("{}{}", self.head, self.children.concat())
    }
}

impl Block<'_> {
    fn text(&self) -> String {
        match self {
            Block::Item(item) => item.text(),
            Block::Line(line) => line.to_string(),
        }
    }
}

/// 節の本文を、一番上の階層の箇条書きとそれ以外の行に分ける。つなげると元の本文に戻る
pub(crate) fn blocks(section: &str) -> Vec<Block<'_>> {
    let mut out: Vec<Block> = Vec::new();
    let mut fenced = false;
    for line in section.split_inclusive('\n') {
        let body = line.trim_end_matches(['\n', '\r']);
        if body.starts_with("```") {
            fenced = !fenced;
            out.push(Block::Line(line));
            continue;
        }
        if fenced {
            out.push(Block::Line(line));
            continue;
        }
        if body.starts_with("- ") || body.starts_with("* ") {
            out.push(Block::Item(Item {
                date: date_of(&body[2..]),
                head: line,
                children: Vec::new(),
            }));
            continue;
        }
        if body.starts_with([' ', '\t'])
            && !body.trim().is_empty()
            && let Some(Block::Item(item)) = out.last_mut()
        {
            item.children.push(line);
            continue;
        }
        out.push(Block::Line(line));
    }
    out
}

/// 箇条書きの本文の先頭にある `YYYY-MM-DD`
fn date_of(text: &str) -> Option<&str> {
    let text = text.strip_prefix("**").unwrap_or(text);
    let date = text.get(..10)?;
    let shaped = date.bytes().enumerate().all(|(i, b)| match i {
        4 | 7 => b == b'-',
        _ => b.is_ascii_digit(),
    });
    shaped.then_some(date)
}

/// 日付つきの箇条書きのうち、日付の新しい `keep` 件の位置（`blocks` の添字）。
/// 同じ日付は、古い順に並んだ節なら後ろ、新しい順に並んだ節なら前を新しいとみなす
pub(crate) fn newest(blocks: &[Block], keep: usize) -> HashSet<usize> {
    let mut dated: Vec<(&str, usize)> = blocks
        .iter()
        .enumerate()
        .filter_map(|(i, block)| match block {
            Block::Item(item) => item.date.map(|date| (date, i)),
            Block::Line(_) => None,
        })
        .collect();
    let ascending = match (dated.first(), dated.last()) {
        (Some(first), Some(last)) => first.0 < last.0,
        _ => false,
    };
    dated.sort_by(|a, b| {
        let by_place = if ascending {
            b.1.cmp(&a.1)
        } else {
            a.1.cmp(&b.1)
        };
        b.0.cmp(a.0).then(by_place)
    });
    dated.into_iter().take(keep).map(|(_, i)| i).collect()
}

/// 要点に出す節の本文。「現状」は日付つきの行を新しい `STATUS_KEEP` 件に、「課題・次の打ち手」は
/// 先頭 `BACKLOG_KEEP` 件にし、1件を `ITEM_CHARS` 字まで・子の行なしにする。
/// 切ったら、箇条書きの後ろに件数と全文の取り方（`path` は README のパス）を添える。ほかの節はそのまま
pub(crate) fn digest_section(title: &str, section: &str, path: &str) -> String {
    if ![STATUS, BACKLOG].contains(&title) {
        return section.to_string();
    }
    let blocks = blocks(section);
    let recent = newest(&blocks, STATUS_KEEP);
    let mut out = String::new();
    let (mut dropped, mut shortened, mut listed) = (0, 0, 0);
    // 絞る対象の箇条書きの終わり。切った件数はここに添える
    let mut end = 0;
    for (i, block) in blocks.iter().enumerate() {
        let item = match block {
            Block::Line(line) => {
                // 落とした行の前後の空行が重ならないようにする
                if !(line.trim().is_empty() && out.ends_with("\n\n")) {
                    out.push_str(line);
                }
                continue;
            }
            Block::Item(item) => item,
        };
        if title == STATUS && item.date.is_none() {
            out.push_str(&item.text());
            continue;
        }
        listed += 1;
        let keep = match title {
            STATUS => recent.contains(&i),
            _ => listed <= BACKLOG_KEEP,
        };
        if keep {
            let (line, cut) = shorten(item);
            shortened += usize::from(cut);
            out.push_str(&line);
        } else {
            dropped += 1;
        }
        end = out.len();
    }
    let note = match (dropped, shortened) {
        (0, 0) => return out,
        (0, cut) => format!("{cut} 件を短くしています"),
        (dropped, 0) => format!("ほか {dropped} 件"),
        (dropped, cut) => format!("ほか {dropped} 件・{cut} 件を短くしています"),
    };
    out.insert_str(
        end,
        &format!("…{note}（全文は ika kb cat {path} --section \"{title}\"）\n"),
    );
    out
}

/// 過去の動きを置くファイル（README と同じディレクトリ）
pub(crate) const HISTORY: &str = "history.md";

/// 「現状」の古い行を移した結果
pub(crate) struct Rotation {
    /// 古い行を除き、`history.md` へのリンクを足した「現状」の節
    pub section: String,
    /// 移す行（子の行つき）。日付の新しい順
    pub moved: Vec<String>,
    /// 残した日付つきの行の件数
    pub kept: usize,
}

/// 「現状」の節（見出しを含む）から、日付の新しい `keep` 件を残して古い日付つきの行を抜く。
/// 行の本文は変えない。移す行が無ければ `moved` は空で、節もそのまま
pub(crate) fn rotate(section: &str, keep: usize) -> Rotation {
    let blocks = blocks(section);
    let recent = newest(&blocks, keep);
    let mut out = String::new();
    let mut moved = Vec::new();
    // 抜いた行の前後の空行が重ならないようにする
    let mut gap = false;
    for (i, block) in blocks.iter().enumerate() {
        match block {
            Block::Line(line) => {
                if !(gap && line.trim().is_empty() && out.ends_with("\n\n")) {
                    out.push_str(line);
                }
                gap &= line.trim().is_empty();
            }
            Block::Item(item) if item.date.is_some() && !recent.contains(&i) => {
                // 節の最後の行で改行が無くても、移した先で次の行とつながらないようにする
                let mut text = item.text();
                if !text.ends_with('\n') {
                    text.push('\n');
                }
                moved.push((item.date.unwrap_or_default(), text));
                gap = true;
            }
            Block::Item(item) => {
                out.push_str(&item.text());
                gap = false;
            }
        }
    }
    let kept = recent.len();
    if moved.is_empty() {
        return Rotation {
            section: section.to_string(),
            moved: Vec::new(),
            kept,
        };
    }
    // 古い順に並んだ節は、同じ日付の中も下が新しい。新しい順にそろえる
    let ascending = moved.first().map(|m| m.0) < moved.last().map(|m| m.0);
    if ascending {
        moved.reverse();
    }
    moved.sort_by(|a, b| b.0.cmp(a.0));
    let link = format!("]({HISTORY})");
    if !out.contains(&link) {
        let spaced = out.ends_with("\n\n");
        out.truncate(out.trim_end_matches('\n').len());
        out.push_str(&format!("\n\n過去の動き: [{HISTORY}]({HISTORY})\n"));
        if spaced {
            out.push('\n');
        }
    }
    Rotation {
        section: out,
        moved: moved.into_iter().map(|(_, text)| text).collect(),
        kept,
    }
}

/// `history.md` の本文に `moved` を足す。日付の新しい順に並べ、すでにある行は足さない。
/// 日付つきの行でないもの（冒頭の説明など）は残す。足した件数も返す
pub(crate) fn merge_history(history: &str, moved: &[String]) -> (String, usize) {
    let blocks = blocks(history);
    let dated = |block: &Block| matches!(block, Block::Item(item) if item.date.is_some());
    let existing: Vec<String> = blocks
        .iter()
        .filter_map(|block| match block {
            Block::Item(item) if item.date.is_some() => Some(item.text()),
            _ => None,
        })
        .collect();
    let fresh: Vec<&String> = moved.iter().filter(|m| !existing.contains(m)).collect();
    // 同じ日付なら、後から移した行のほうが新しい
    let mut items: Vec<&str> = fresh.iter().map(|m| m.as_str()).collect();
    items.extend(existing.iter().map(String::as_str));
    let date = |text: &str| date_of(&text[2..]).unwrap_or_default().to_string();
    items.sort_by_key(|text| std::cmp::Reverse(date(text)));

    let first = blocks.iter().position(dated).unwrap_or(blocks.len());
    let mut out = String::new();
    for block in &blocks[..first] {
        out.push_str(&block.text());
    }
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    if first == blocks.len() && !out.is_empty() && !out.ends_with("\n\n") {
        out.push('\n');
    }
    for item in items {
        out.push_str(item);
        if !item.ends_with('\n') {
            out.push('\n');
        }
    }
    for block in blocks[first..].iter().filter(|block| !dated(block)) {
        out.push_str(&block.text());
    }
    (out, fresh.len())
}

/// `ika check` が知らせる「現状」の件数（これを超えたら）
const WARN_ITEMS: usize = 10;
/// `ika check` が知らせる「現状」の1件の字数（これを超えたら）
const WARN_CHARS: usize = 300;

/// README の「現状」「課題・次の打ち手」の伸び。`ika check` の注意に使う
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Growth {
    /// 「現状」の日付つきの行の件数
    pub dated: usize,
    /// そのうち `WARN_CHARS` 字を超える行の件数
    pub long: usize,
    /// 「課題・次の打ち手」の、打ち消し線のある行の数
    pub struck: usize,
}

impl Growth {
    /// 知らせることがあれば、その中身（件数の並び）
    pub(crate) fn notice(&self) -> Option<String> {
        let mut parts = Vec::new();
        if self.dated > WARN_ITEMS {
            parts.push(format!(
                "「現状」の日付つきの行 {} 件（{WARN_ITEMS} 件まで）",
                self.dated
            ));
        }
        if self.long > 0 {
            parts.push(format!(
                "{WARN_CHARS} 字を超える「現状」の行 {} 件",
                self.long
            ));
        }
        if self.struck > 0 {
            parts.push(format!("打ち消し線の残る課題 {} 行", self.struck));
        }
        (!parts.is_empty()).then(|| parts.join("・"))
    }

    /// `ika project rotate` で古い行を移せるか
    pub(crate) fn rotatable(&self) -> bool {
        self.dated > STATUS_KEEP && (self.dated > WARN_ITEMS || self.long > 0)
    }
}

/// 「現状」と「課題・次の打ち手」の節（どちらも無ければ空文字）から伸びを数える
pub(crate) fn growth(status: &str, backlog: &str) -> Growth {
    let mut growth = Growth::default();
    for block in blocks(status) {
        if let Block::Item(item) = block
            && item.date.is_some()
        {
            growth.dated += 1;
            let chars = item.text().trim_end().chars().count();
            growth.long += usize::from(chars > WARN_CHARS);
        }
    }
    growth.struck = backlog.lines().filter(|line| line.contains("~~")).count();
    growth
}

/// 箇条書き1件を1行にする。長ければ `ITEM_CHARS` 字で切り、子の行は行数だけ添える。
/// 2つ目の値は、切った・畳んだかどうか
fn shorten(item: &Item) -> (String, bool) {
    let body = item.head.trim_end_matches(['\n', '\r']);
    let mut line: String = body.chars().take(ITEM_CHARS).collect();
    let long = line.len() < body.len();
    if long {
        line.push('…');
    }
    if !item.children.is_empty() {
        line.push_str(&format!("（＋{} 行）", item.children.len()));
    }
    line.push('\n');
    (line, long || !item.children.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATH: &str = "kb/projects/foo/README.md";

    fn dated(dates: &[&str]) -> String {
        let mut section = String::from("## 現状\n\n公開済み。\n\n");
        for date in dates {
            section.push_str(&format!("- {date}: {date} の動き\n"));
        }
        section.push_str("\nスコープ:\n\n- Web\n  - 記録\n\n");
        section
    }

    #[test]
    fn blocks_join_back_to_the_section() {
        let section = "## 現状\n\n段落\n\n- 2026-11-01: a\n  - 子\n\n  離れた字下げ\n- **2026-11-02**: b\n```\n- コードの中\n```\n* 日付なし\n";
        let blocks = blocks(section);
        let joined: String = blocks
            .iter()
            .map(|block| match block {
                Block::Line(line) => line.to_string(),
                Block::Item(item) => format!("{}{}", item.head, item.children.concat()),
            })
            .collect();
        assert_eq!(joined, section);
        let items: Vec<(Option<&str>, usize)> = blocks
            .iter()
            .filter_map(|block| match block {
                Block::Item(item) => Some((item.date, item.children.len())),
                Block::Line(_) => None,
            })
            .collect();
        assert_eq!(
            items,
            [(Some("2026-11-01"), 1), (Some("2026-11-02"), 0), (None, 0)]
        );
    }

    #[test]
    fn status_keeps_the_newest_dated_items_in_place() {
        let old_first = [
            "2026-10-01",
            "2026-10-02",
            "2026-10-03",
            "2026-11-01",
            "2026-11-02",
            "2026-11-03",
            "2026-11-04",
            "2026-11-05",
        ];
        let mut new_first = old_first;
        new_first.reverse();
        for dates in [old_first, new_first] {
            let out = digest_section(STATUS, &dated(&dates), PATH);
            let shown: Vec<&str> = dates
                .iter()
                .copied()
                .filter(|date| out.contains(&format!("{date} の動き")))
                .collect();
            let newest: Vec<&str> = dates
                .iter()
                .copied()
                .filter(|date| date.starts_with("2026-11"))
                .collect();
            assert_eq!(shown, newest, "{out}");
            let places: Vec<usize> = shown.iter().map(|date| out.find(date).unwrap()).collect();
            assert!(places.is_sorted(), "元の並びのまま: {out}");
            // 日付のない行（冒頭の段落・後ろの塊と子の行）はそのまま
            assert!(
                out.starts_with("## 現状\n\n公開済み。\n\n- 2026-11"),
                "{out}"
            );
            assert!(
                out.ends_with(
                    "\n…ほか 3 件（全文は ika kb cat kb/projects/foo/README.md --section \"現状\"）\n\nスコープ:\n\n- Web\n  - 記録\n\n"
                ),
                "{out}"
            );
        }
    }

    #[test]
    fn same_day_items_follow_the_order_of_the_section() {
        let numbered = |dates: &[&str]| -> String {
            let mut section = String::from("## 現状\n\n");
            for (i, date) in dates.iter().enumerate() {
                section.push_str(&format!("- {date}: 項目{i}\n"));
            }
            section
        };
        // 新しい順の節: 同じ日は上が新しい
        let mut dates = vec!["2026-11-02"; 7];
        dates.push("2026-11-01");
        let out = digest_section(STATUS, &numbered(&dates), PATH);
        assert!(out.contains("項目0") && out.contains("項目4"), "{out}");
        assert!(!out.contains("項目5") && !out.contains("項目7"), "{out}");
        // 古い順の節: 同じ日は下が新しい
        let mut dates = vec!["2026-11-01"];
        dates.extend(["2026-11-02"; 7]);
        let out = digest_section(STATUS, &numbered(&dates), PATH);
        assert!(out.contains("項目3") && out.contains("項目7"), "{out}");
        assert!(!out.contains("項目0") && !out.contains("項目2"), "{out}");
    }

    #[test]
    fn backlog_keeps_the_first_items() {
        let mut section = String::from("## 課題・次の打ち手\n\n以下は案で、未決定。\n\n");
        for i in 1..=25 {
            section.push_str(&format!("- 課題{i:02}\n"));
        }
        section.push_str("\n後ろの段落\n");
        let out = digest_section(BACKLOG, &section, PATH);
        assert!(out.contains("以下は案で、未決定。") && out.contains("後ろの段落"));
        assert!(
            out.contains("- 課題20\n") && !out.contains("課題21"),
            "{out}"
        );
        assert!(
            out.contains(
                "\n…ほか 5 件（全文は ika kb cat kb/projects/foo/README.md --section \"課題・次の打ち手\"）\n"
            ),
            "{out}"
        );
    }

    #[test]
    fn long_items_are_cut_and_children_are_folded() {
        let long = "あ".repeat(198);
        let section = format!(
            "## 課題・次の打ち手\n\n- {long}\n- 親\n  - 子1\n  - 子2\n    - 孫\n- 短い\n- ~~済み~~: 残り\n"
        );
        let out = digest_section(BACKLOG, &section, PATH);
        let cut = format!("- {}…\n", "あ".repeat(158));
        assert!(out.contains(&cut), "160 字で切る: {out}");
        assert!(
            out.contains("- 親（＋3 行）\n") && !out.contains("子1"),
            "{out}"
        );
        assert!(out.contains("- 短い\n- ~~済み~~: 残り\n"), "{out}");
        assert!(out.contains("…2 件を短くしています（全文は "), "{out}");

        let status =
            format!("## 現状\n\n- 2026-11-01: {long}\n  - 子\n- 日付なし {long}\n  - 子\n");
        let out = digest_section(STATUS, &status, PATH);
        assert!(out.contains("…（＋1 行）\n"), "{out}");
        assert!(
            out.contains(&format!("- 日付なし {long}\n  - 子\n")),
            "日付のない箇条書きはそのまま: {out}"
        );
    }

    #[test]
    fn rotate_moves_old_dated_items_and_keeps_the_rest() {
        let section = "## 現状\n\n公開済み。\n\n- 2026-10-01: 一\n  - 子\n- 2026-10-02: 二\n- 2026-11-01: 三\n- 2026-11-02: 四\n- 日付なし\n\nスコープ:\n\n- Web\n\n";
        let rotation = rotate(section, 2);
        assert_eq!(rotation.kept, 2);
        // 移す行は新しい順。子の行も一緒で、本文は変えない
        assert_eq!(
            rotation.moved,
            ["- 2026-10-02: 二\n", "- 2026-10-01: 一\n  - 子\n"]
        );
        assert_eq!(
            rotation.section,
            "## 現状\n\n公開済み。\n\n- 2026-11-01: 三\n- 2026-11-02: 四\n- 日付なし\n\nスコープ:\n\n- Web\n\n過去の動き: [history.md](history.md)\n\n"
        );
        // もう一度かけても変わらない（リンクも増えない）
        let again = rotate(&rotation.section, 2);
        assert!(again.moved.is_empty());
        assert_eq!(again.section, rotation.section);
        // リンクがすでにあれば足さない
        let next = rotation
            .section
            .replace("- 日付なし\n", "- 2026-11-03: 五\n");
        let rotated = rotate(&next, 2);
        assert_eq!(rotated.moved, ["- 2026-11-01: 三\n"]);
        assert_eq!(rotated.section.matches("](history.md)").count(), 1);
    }

    #[test]
    fn rotate_picks_the_same_items_in_any_order() {
        let dates = ["2026-10-01", "2026-11-03", "2026-10-02", "2026-11-01"];
        let section = |dates: &[&str]| {
            let lines: String = dates.iter().map(|d| format!("- {d}: x\n")).collect();
            format!("## 現状\n\n{lines}")
        };
        let mut sorted = dates;
        sorted.sort();
        let mut reversed = sorted;
        reversed.reverse();
        for order in [dates, sorted, reversed] {
            let rotation = rotate(&section(&order), 2);
            assert_eq!(
                rotation.moved,
                ["- 2026-10-02: x\n", "- 2026-10-01: x\n"],
                "{order:?}"
            );
            assert!(
                rotation.section.contains("2026-11-03") && rotation.section.contains("2026-11-01")
            );
        }
        // 最後の行に改行が無くても、節が README の末尾でも壊れない
        let rotation = rotate("## 現状\n\n- 2026-11-02: 新\n- 2026-11-01: 旧", 1);
        assert_eq!(rotation.moved, ["- 2026-11-01: 旧\n"]);
        assert_eq!(
            rotation.section,
            "## 現状\n\n- 2026-11-02: 新\n\n過去の動き: [history.md](history.md)\n"
        );
    }

    #[test]
    fn history_is_merged_newest_first_without_duplicates() {
        let head = "---\ntitle: foo の過去の動き\n---\n\n説明。\n";
        let (first, added) = merge_history(
            head,
            &[
                "- 2026-10-02: 二\n".into(),
                "- 2026-10-01: 一\n  - 子\n".into(),
            ],
        );
        assert_eq!(added, 2);
        assert_eq!(
            first,
            format!("{head}\n- 2026-10-02: 二\n- 2026-10-01: 一\n  - 子\n")
        );
        // 後から移した行は上に入る。すでにある行は足さず、日付のない行は残す
        let noted = format!("{first}\n手で足したメモ\n");
        let (second, added) = merge_history(
            &noted,
            &[
                "- 2026-11-01: 三".into(),
                "- 2026-10-02: 二\n".into(),
                "- 2026-09-30: 零\n".into(),
            ],
        );
        assert_eq!(added, 2);
        assert_eq!(
            second,
            format!(
                "{head}\n- 2026-11-01: 三\n- 2026-10-02: 二\n- 2026-10-01: 一\n  - 子\n- 2026-09-30: 零\n\n手で足したメモ\n"
            )
        );
    }

    #[test]
    fn growth_counts_long_status_and_struck_backlog() {
        let line = |date: &str, chars: usize| format!("- {date}: {}\n", "あ".repeat(chars));
        let mut status = String::from("## 現状\n\n公開済み。\n\n- 日付なし\n");
        for day in 1..=10 {
            status.push_str(&line(&format!("2026-11-{day:02}"), 286));
        }
        let quiet = growth(&status, "## 課題・次の打ち手\n\n- 次\n");
        assert_eq!(quiet.dated, 10);
        assert_eq!(quiet.notice(), None, "10 件・300 字までは知らせない");

        status.push_str(&line("2026-11-11", 287));
        let grown = growth(
            &status,
            "## 課題・次の打ち手\n\n- ~~済み~~: 残り\n- 次\n  - ~~子~~\n",
        );
        assert_eq!(
            grown,
            Growth {
                dated: 11,
                long: 1,
                struck: 2
            }
        );
        let notice = grown.notice().unwrap();
        for part in [
            "11 件（10 件まで）",
            "300 字を超える「現状」の行 1 件",
            "課題 2 行",
        ] {
            assert!(notice.contains(part), "{notice}");
        }
        assert!(grown.rotatable());
        // 件数が少なければ、長い行があっても移すものが無い
        let few = growth(&format!("## 現状\n\n{}", line("2026-11-01", 400)), "");
        assert!(few.notice().is_some() && !few.rotatable());
    }

    #[test]
    fn untouched_sections_get_no_note() {
        let status = dated(&["2026-11-01", "2026-11-02"]);
        assert_eq!(digest_section(STATUS, &status, PATH), status);
        let backlog = "## 課題・次の打ち手\n\n- 1つ\n- 2つ\n";
        assert_eq!(digest_section(BACKLOG, backlog, PATH), backlog);
        let other = format!("## 技術\n\n- {}\n  - 子\n", "あ".repeat(300));
        assert_eq!(digest_section("技術", &other, PATH), other);
    }
}
