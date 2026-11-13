# kb — ナレッジベースの書式

フロントマター・ファイル名・リンクの書き方。`ika new` がパスとフロントマターを作り、`ika check` が検証するので、指摘が出たとき・CLI なしで書くときに読む。どの領域・種類に置くかは `kb-layout.md`。

## フロントマター

すべての Markdown の先頭に置く。`ika check` が検証する。
値は1行で書き、リストは `[a, b]` のインライン形式にする（複数行のYAMLは使わない）。
領域はディレクトリで決まるので、フロントマターには書かない。

```yaml
---
title: 内容が分かる一文のタイトル
type: note            # project | spec | note | insight | decision | source
date: 2026-10-14      # 作成日。insight と project は最終更新日として使う
reviewed: 2026-10-20  # 任意。確かめ直した日（ika kb review）。stale は date と遅いほうを基準にする
projects: [foo]       # data/projects.csv の name。横断的な内容なら複数、無関係なら []
tags: [foo, bar-baz]  # 小文字・ハイフン区切り
source: https://...   # 任意。出典URL
status: accepted      # decision と spec のみ必須。decision: proposed | accepted | superseded | rejected
                      # spec: draft | approved | done | dropped
# source のみ（ika import が書く）: source（原本の URL かパス。必須）、format（md・docx・pdf など）、hash（原本の SHA-256）
---
```

## 種類と置き場所

| type | 置き場所 | ファイル名 | 性質 |
|---|---|---|---|
| project | `projects/<name>/README.md` | 固定 | プロダクトの現状。変わったら更新する |
| note | `<領域>/notes/YYYY/` | `YYYY-MM-DD-<slug>.md` | 情報収集・調査・ログ。原則追記のみ |
| insight | `<領域>/insights/` | `<slug>.md` | 蒸留した知見。根拠の note へリンクし、育てる |
| decision | `<領域>/decisions/` | `YYYY-MM-DD-<slug>.md` | 意思決定。覆したら新しい decision を作り、古い方を `superseded` にする |
| source | `projects/<name>/sources/`・`<領域>/sources/` | `<slug>.md` | 外部のドキュメント（仕様書・Drive・Notion・docx・xlsx・pptx・pdf・html）の全文。`ika import` で作り、同じ `source` で取り込み直すと上書きされる。直接編集せず、要点は README・要件書・ノートに書いてリンクする。原本のリンクを前提にしているので、本文のリンク切れは `ika check` で確かめない |
| spec | `projects/<name>/specs/` | `<slug>.md` | 要件書。機能1つ分の目的・範囲・受け入れ条件。`draft`（承認待ち）→ `approved` → `done`（実装済み。作った形に直す）、やめたら `dropped`。`projects` は置き場所のプロダクト1つ |

プロダクト固有の資料が増えたら `projects/<name>/` の下に置いてよい（type は内容に合わせる）。

## 図

流れ・構成・データの形・経緯・案の比較は図にする。言語名つきのコードブロック（` ```seq `・` ```flow ` など。Mermaid は使わない）で書き、記法は [diagrams.md](diagrams.md)。

## リンク

ノート間は相対パスの Markdown リンクで張る（例: `[根拠](../notes/2026/2026-10-14-foo.md)`）。
Obsidian 等の `[[...]]` 記法は、ChatGPT や Codex で解決できないので使わない。
