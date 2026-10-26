# kb — ナレッジベースの書式

## 置き場所

ナレッジのデータは ika のリポジトリには置かず、環境変数 `IKA_HOME` で指定したディレクトリに置く。
中は `kb/`（このファイルが書式を定めるノート）と `data/`（`projects.csv`・`domains.csv`）。
このファイルの `kb/…`・`data/…` のパスは、すべて `IKA_HOME` からの相対パス。作成は `ika init`、読み書きは `ika` コマンドで行う。
`IKA_HOME` は手元だけの git リポジトリで、`ika` の書き込みごとに自動でコミットされる（リモートは無い）。

## 構成

ナレッジは**領域**ごとにディレクトリを分ける。領域の一覧は `data/domains.csv` が正本。

```
kb/
  business/                ビジネス・ドメイン知識
    notes/YYYY/  insights/  decisions/
  tech/                    技術
    notes/YYYY/  insights/  decisions/
  projects/<name>/README.md  プロダクト概要（ビジネスと技術の両面をまとめる。領域には属さない）
  projects/<name>/specs/     要件書（機能ごと。cycle スキルが作り、実装したら今の形に直す）
  projects/<name>/sources/   取り込んだ外部の資料の全文（ika import が作る。直接編集しない）
  <領域>/sources/            プロダクトに属さない外部の資料の全文（市場レポートなど）
```

### どちらの領域に置くか

「そのナレッジを何の判断に使うか」で決める。

| 領域 | 置くもの | 例 |
|---|---|---|
| `business` | 何を作るか・誰に届けるか・どう稼ぐか。市場・顧客・業界の制度や慣習・競合・価格・集客・法務 | 競合の料金体系、ユーザーインタビュー、業界の規制・制度の要件、集客経路ごとの傾向 |
| `tech` | どう作るか・どう動かすか。設計・実装・インフラ・ツール・運用・技術的なコスト・セキュリティ | フレームワークの移行手順、データベースの設計パターン、認証の実装メモ、インフラ費用の内訳 |

両方にまたがるもの（例: インフラ費用が価格設定に効く）は、主に使う側に置き、もう一方の関連ノートから相対リンクを張る。
領域を足すときは `ika domain add <name> --summary "..."`（`data/domains.csv` に1行足され、`kb/<name>/` ができる）。

## フロントマター

すべての Markdown の先頭に置く。`ika check` が検証する。
値は1行で書き、リストは `[a, b]` のインライン形式にする（複数行のYAMLは使わない）。
領域はディレクトリで決まるので、フロントマターには書かない。

```yaml
---
title: 内容が分かる一文のタイトル
type: note            # project | spec | note | insight | decision | source
date: 2026-10-14      # 作成日。insight と project は最終更新日として使う
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

### どこに書くか

| 書くこと | 置き場所 |
|---|---|
| 起きたこと・調べたこと（その時点のログ） | note |
| **今の状態**（設計・仕様・スキーマ・ドメインモデル。変わると古くなる） | `projects/<name>/README.md`、またはその下の資料。更新して育てる |
| 複数のノートから言える一般化された知見 | insight。ノートのコピーではなく、AI が書き直して一文に絞り、根拠のノートへリンクする |
| 決めたこと | decision |
| 外部のドキュメントそのもの | source（`ika import`）。全文は写しとして置き、判断に使う要点は上のどれかに書き起こす |
| これから作るもの・作ったものの要件 | spec（`ika new spec "タイトル" --project <name> --slug <英字>`）。方式の選択など判断が要る点は decision にして要件書からリンクする |

まだ領域もプロダクトも決められないメモは、`ika capture` で `IKA_HOME/inbox/YYYY-MM-DD.md`（`kb/` の外。フロントマターなし）に溜めておき、後でノートに振り分ける。

ノートは溜めてよいが、溜まったら `ika kb orphans` で、どの insight・decision・プロダクトの資料（README を除く）からもリンクされていないノートを確認し、insight に蒸留する。README の「関連」は索引なので、整理済みとは数えない。

## 図

流れ・構成・データの形・経緯・案の比較は、文章より図のほうが速く伝わる。図は言語名つきのコードブロックで書き、`ika export html` と `ika serve` が CSS だけで描く（Mermaid は使わない）。GitHub などではただのコードブロックとして読める。

- 1行が1要素。`#` で始まる行はコメント。区切りの `:` は全角の `：` でもよい
- 言語名のあとの文は図の説明になる（` ```seq 打刻の流れ `）
- 記法の誤りは `ika check` が行番号つきで指摘し、`ika export html` は書き出さずに止まる
- 自由に箱を置いて線を引く図（任意のグラフ）は描けない。構造で配置が決まる次の7種だけ

| 言語名 | 図 | 書き方 |
|---|---|---|
| `seq` | シーケンス図 | `A -> B: 文`。`->` 呼び出し、`-->` 返り値（破線）、`=>` 強調。参加者は登場順に左から。`note A: 文` で注記 |
| `flow` | フロー・状態遷移 | 1行目に `a -> b -> ?判断`。判断のあとに字下げで `ラベル: c -> d` の枝。印は `?` 判断、`*` 主役、`+` 成功、`!` 問題。`名前 \| 補足`。合流・ループは描かない |
| `layers` | 層の構成図 | `名前 \| 補足 {new}: 札, 札`。上の段が下の段を使う。`{new}` 藍、`{keep}` 苔 |
| `er` | ER 図 | `表示名 英名` の下に字下げで `*主キー: 型`・`属性: 型`。関係は `A 1 -- 0..* B: 関係名`。線は隣り合う箱だけ（ほかは図の下に文字で出る） |
| `class` | クラス図（継承の木） | `子 : 親 <<abstract>>` の下に字下げで属性・操作、`--` で区画を分ける。関連は描かない |
| `timeline` | 年表 | `2026-10-23: 文`（`YYYY-MM-DD` か `YYYY-MM` だけ）。`? 未定: 文` は未定の項目 |
| `compare` | 案の比較 | `名前 \| 説明` の下に字下げで `+ 長所`・`- 短所`・`= 結論`。採用した案だけ先頭に `*`（未決定なら付けない） |

````````````markdown
```flow 申請の流れ
下書き -> 申請 -> ?承認する？
  はい: +承認済み
  いいえ: !差し戻し
```
````````````

コールアウトは GitHub と同じ `> [!NOTE]`・`> [!TIP]`・`> [!IMPORTANT]`・`> [!WARNING]`・`> [!CAUTION]` で書く。

## リンク

ノート間は相対パスの Markdown リンクで張る（例: `[根拠](../notes/2026/2026-10-14-foo.md)`）。
Obsidian 等の `[[...]]` 記法は、ChatGPT や Codex で解決できないので使わない。

## プロダクトの追加

```bash
ika project add <name> --path <リポジトリのパス> --summary "一行説明"
```

`data/projects.csv` に行が足され、`projects/<name>/README.md` がテンプレートから作られる。
