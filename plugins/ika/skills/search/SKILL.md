---
name: search
description: ika のナレッジベース（プロダクト概要・調査メモ・知見・意思決定ログ）を検索して答える。「前に調べた〜は？」「〜について何を決めたっけ」「KBにある？」「過去の知見を踏まえて」で使う。製品リポジトリ（ika 自身を除く）には ika・KB への言及を書かない。
compatibility: KB（環境変数 IKA_HOME のディレクトリ）の読み書きには ika CLI が必要。Claude Code・Codex・ChatGPT デスクトップアプリ向け
---

# search: ナレッジを検索して答える

KB は ika のリポジトリではなく環境変数 `IKA_HOME` のディレクトリにある。未設定だと `ika` は動かないので、設定を頼む。読み書きは `ika` CLI で行い、ファイルを直接 Read/Write/rg しない。構成と書式は [references/kb-format.md](references/kb-format.md)。
製品リポジトリには ika・KB への言及を書かない（ika 自身のリポジトリを除く）。コード・コメント・テスト名・ドキュメント・設定・コミットメッセージ・PR に、ika・KB・`kb/` のパス・要件書を書かず、出典は一次情報の URL だけにする。詳しくは [cycle の資料](../cycle/references/product-repo.md)。
KB にアクセスできない環境では、プロジェクトに上げられたナレッジファイル（`knowledge-*.md`。各ノートの先頭に `<!-- path: ... -->` がある）を同じ手順で探す。

## 手順

1. 対象プロダクトが話題にあれば `ika project list` で `name` を確かめ、`ika project show <name> --readme` で概要（README）と関連する決定・知見・ノートの一覧を一度に見る
2. 領域を絞る。ビジネス・ドメインの問いなら `-d business`、技術の問いなら `-d tech`（一覧は `ika domain list`）。当たらなければ領域を外して全体に広げる
3. `ika kb search` で絞り込む。検索語は部分一致・大文字小文字無視で、日本語は表記ゆれ（カタカナ／英語、略称）を `--any` で並べる
   ```bash
   ika kb search 'review' 'レビュー' --any -d tech -l      # どれかを含むファイル。-l でファイル名のみ
   ika kb search 'review' 'レビュー' -p <name>             # すべて含むファイル。一致した行も出る
   ika kb search -t <tag> -d business -l           # タグで絞る（検索語なしでもよい）
   ika kb search --type decision --status accepted -l   # 有効な決定
   ika kb search --type spec -p <name> -l            # 要件書（status: draft 承認待ち / approved / done 実装済み / dropped）
   ika kb search --type source -p <name> -l          # 取り込んだ外部の資料の全文（仕様書・Drive・Notion など。原本は source:）
   ika kb cat kb/tech/decisions/<file>.md            # 全文を読む（8000字を超えるファイルは見出し一覧だけ出る。全文は --full）
   ika kb cat <path> --outline                       # 長いファイルは先に見出し一覧（各節の行範囲・文字数つき）を見る
   ika kb cat <path> --section "技術"                # 必要な節だけ読む（同名の見出しがあれば "## 技術" と階層を付ける）
   ika kb cat <path> --lines 40:80                   # grep -n で見つけた行の周辺だけ読む（--frontmatter でフロントマターのみ）
   ika kb grep -n -C2 'review|レビュー' kb/tech           # 正規表現・前後の行など自由に探す。中身は ripgrep（オプションは許可されたものだけ）
   ```
   `ika kb search` は既定で20ファイル（`-l` と検索語なしは50）・1ファイル3行までに絞る。標準エラーに「N 件（上位 M 件を表示）」と出たら、後ろが切れている。`-p`・`-d`・`--type`・`-t` で絞るか、`-n` を増やす（`-n 0` で全件）。「KBに記録なし」と答えるのは、切れていないことを確かめてから
   `ika kb search` で足りない探し方（正規表現、前後の行、単語境界など）は `ika kb grep` を使う。検索範囲は kb/ の中に限られる。結果は信頼度順にならないので、当たったファイルの `type`・`status` は自分で確かめる
   CLI が使えない環境でだけ、`IKA_HOME` 下の `kb/` を `rg` で直接探す
4. 数値や推移を問われたら、表形式データも見る。`ika data ls` で一覧し、`ika data show <name>` で列を確かめ、`ika data sql "select ..."` で集計する（SELECT のみ。テーブル名はデータセット名）。ノートや知見がデータセットを引用していることもある
5. 検索結果は信頼度順に並ぶ（採用済みの決定 → 知見 → プロダクト概要・承認済みと実装済みの要件 → 案の決定・下書きの要件 → ノート・取り込んだ資料 → 覆された決定・取り下げた要件。同じ種類では新しい日付が先）。取り込んだ資料（`type: source`）は原本の写しで、日付は取り込んだ日。古いかもしれないときは原本（`source:`）を示す。上から読み、ノートは一次情報・未検証のメモとして扱う。`superseded` は経緯としてだけ使う
6. 答える
   - 根拠にしたファイルのパス（データなら、データセット名と実行した SQL）を必ず添える
   - KB に無いことは「KBに記録なし」と明言し、一般知識で補うならそうと分かるように書く
   - 根拠にしたノート・知見ごとに、最後に確かめた日（フロントマターの `reviewed` があればそれ、無ければ `date`。`ika kb search` の行には `確認 日付` と出る）と、今日から何日前かを添える。価格・競合など変わりやすい情報で 90 日を超えていれば「古い可能性がある」と書き、確かめ直しを提案する（`capture` スキルの [確かめ直し](../capture/references/review.md)）
   - 記録同士が矛盾していたら、両方示して指摘する
   - ビジネスと技術の両方に根拠があるときは、どちらの領域の話かを分けて示す
7. 調べた結果、新しく分かったことがあれば `capture` スキルでの記録を提案する
