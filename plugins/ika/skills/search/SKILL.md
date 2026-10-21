---
name: search
description: ika のナレッジベース（プロダクト概要、ビジネス・ドメイン知識と技術の調査メモ・知見・意思決定ログ）を検索して答える。「前に調べた〜は？」「〜について何を決めたっけ」「KBにある？」「過去の知見を踏まえて」のような質問や、意思決定の前に過去の経緯を確認したいときに使う。
compatibility: KB（環境変数 IKA_HOME のディレクトリ）の読み書きには ika CLI が必要。Claude Code・Codex・ChatGPT デスクトップアプリ向け
---

# search: ナレッジを検索して答える

KB は ika のリポジトリではなく環境変数 `IKA_HOME` のディレクトリにある。未設定だと `ika` は動かないので、設定を頼む。読み書きは `ika` CLI で行い、ファイルを直接 Read/Write/rg しない。構成と書式は [references/kb-format.md](references/kb-format.md)。
KB にアクセスできない環境では、プロジェクトに上げられたナレッジファイル（`knowledge-*.md`。各ノートの先頭に `<!-- path: ... -->` がある）を同じ手順で探す。

## 手順

1. 対象プロダクトが話題にあれば `ika project list` で `name` を確かめ、`ika project show <name> --readme` で概要（README）と関連する決定・知見・ノートの一覧を一度に見る
2. 領域を絞る。ビジネス・ドメインの問いなら `-d business`、技術の問いなら `-d tech`（一覧は `ika domain list`）。当たらなければ領域を外して全体に広げる
3. まず `ika kb fts` で探す。日本語の形態素解析つきの全文検索で、問いの文のままでも語でも引け、関連の強い順に並ぶ。助詞や活用の違い（「取り下げたい」と「取り下げ」）は吸収し、2文字以上なら語の途中でも当たる。言い換え（カタカナ／英語、略称）は当たらないので `--any` で並べる
   ```bash
   ika kb fts 申請を取り下げたい -p <name>           # 文で引ける。一致した行も出る（上位20件。-n で増やす）
   ika kb fts review レビュー --any -d tech -l       # 言い換えを並べる（どれかに当たるファイル）。-l でファイル名のみ
   ika kb fts -t <tag> -d business -l              # 検索語なしは一覧（全件・新しい日付順）
   ika kb fts --type decision --status accepted -l   # 有効な決定
   ika kb fts --type spec -p <name> -l               # 要件書（status: draft 承認待ち / approved / done 実装済み / dropped）
   ika kb fts --type source -p <name> -l             # 取り込んだ外部の資料の全文（仕様書・Drive・Notion など。原本は source:）
   ika kb cat kb/tech/decisions/<file>.md            # 全文を読む
   ika kb cat <path> --outline                       # 長いファイルは先に見出し一覧（各節の行範囲・文字数つき）を見る
   ika kb cat <path> --section "技術"                # 必要な節だけ読む（同名の見出しがあれば "## 技術" と階層を付ける）
   ika kb cat <path> --lines 40:80                   # grep -n で見つけた行の周辺だけ読む（--frontmatter でフロントマターのみ）
   ika kb grep -n -C2 'review|レビュー' kb/tech           # 正規表現・前後の行など自由に探す。中身は ripgrep（オプションは許可されたものだけ）
   ```
   `ika kb fts` で当たらないときは、言い換えを `--any` で足すか、`ika kb search`（文字列をそのまま含むファイルを信頼度順に出す。オプションは同じ）でも探す。`ika kb fts` が無い古い ika（`unrecognized subcommand` と出る）では `ika kb search` を使う
   正規表現・前後の行・単語境界などが要る探し方は `ika kb grep` を使う。検索範囲は kb/ の中に限られる。結果は信頼度順にならないので、当たったファイルの `type`・`status` は自分で確かめる
   CLI が使えない環境でだけ、`IKA_HOME` 下の `kb/` を `rg` で直接探す
4. 数値や推移を問われたら、表形式データも見る。`ika data ls` で一覧し、`ika data show <name>` で列を確かめ、`ika data sql "select ..."` で集計する（SELECT のみ。テーブル名はデータセット名）。ノートや知見がデータセットを引用していることもある
5. `ika kb fts` の結果は関連度順で、信頼度順ではない。各ファイルの `[種類 status]` を見て、信頼度の高いものを根拠の中心にする（採用済みの決定 → 知見 → プロダクト概要・承認済みと実装済みの要件 → 案の決定・下書きの要件 → ノート・取り込んだ資料 → 覆された決定・取り下げた要件。同じ種類では新しい日付が先。`ika kb search` はこの順で並ぶ）。上位に決定・知見が無くても、関わる決定がないか `--type decision -p <name>` で確かめる。取り込んだ資料（`type: source`）は原本の写しで、日付は取り込んだ日。古いかもしれないときは原本（`source:`）を示す。上から読み、ノートは一次情報・未検証のメモとして扱う。`superseded` は経緯としてだけ使う
6. 答える
   - 根拠にしたファイルのパス（データなら、データセット名と実行した SQL）を必ず添える
   - KB に無いことは「KBに記録なし」と明言し、一般知識で補うならそうと分かるように書く
   - 記録同士が矛盾していたら、両方示して指摘する
   - ビジネスと技術の両方に根拠があるときは、どちらの領域の話かを分けて示す
7. 調べた結果、新しく分かったことがあれば `capture` スキルでの記録を提案する
