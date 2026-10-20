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
type: note            # project | spec | note | insight | decision
date: 2026-10-14      # 作成日。insight と project は最終更新日として使う
projects: [foo]       # data/projects.csv の name。横断的な内容なら複数、無関係なら []
tags: [foo, bar-baz]  # 小文字・ハイフン区切り
source: https://...   # 任意。出典URL
status: accepted      # decision と spec のみ必須。decision: proposed | accepted | superseded | rejected
                      # spec: draft | approved | done | dropped
---
```

## 種類と置き場所

| type | 置き場所 | ファイル名 | 性質 |
|---|---|---|---|
| project | `projects/<name>/README.md` | 固定 | プロダクトの現状。変わったら更新する |
| note | `<領域>/notes/YYYY/` | `YYYY-MM-DD-<slug>.md` | 情報収集・調査・ログ。原則追記のみ |
| insight | `<領域>/insights/` | `<slug>.md` | 蒸留した知見。根拠の note へリンクし、育てる |
| decision | `<領域>/decisions/` | `YYYY-MM-DD-<slug>.md` | 意思決定。覆したら新しい decision を作り、古い方を `superseded` にする |
| spec | `projects/<name>/specs/` | `<slug>.md` | 要件書。機能1つ分の目的・範囲・受け入れ条件。`draft`（承認待ち）→ `approved` → `done`（実装済み。作った形に直す）、やめたら `dropped`。`projects` は置き場所のプロダクト1つ |

プロダクト固有の資料が増えたら `projects/<name>/` の下に置いてよい（type は内容に合わせる）。

### どこに書くか

| 書くこと | 置き場所 |
|---|---|
| 起きたこと・調べたこと（その時点のログ） | note |
| **今の状態**（設計・仕様・スキーマ・ドメインモデル。変わると古くなる） | `projects/<name>/README.md`、またはその下の資料。更新して育てる |
| 複数のノートから言える一般化された知見 | insight。ノートのコピーではなく、AI が書き直して一文に絞り、根拠のノートへリンクする |
| 決めたこと | decision |
| これから作るもの・作ったものの要件 | spec（`ika new spec "タイトル" --project <name> --slug <英字>`）。方式の選択など判断が要る点は decision にして要件書からリンクする |

ノートは溜めてよいが、溜まったら `ika kb orphans` で、どの insight・decision・プロダクトの資料（README を除く）からもリンクされていないノートを確認し、insight に蒸留する。README の「関連」は索引なので、整理済みとは数えない。

## リンク

ノート間は相対パスの Markdown リンクで張る（例: `[根拠](../notes/2026/2026-10-14-foo.md)`）。
Obsidian 等の `[[...]]` 記法は、ChatGPT や Codex で解決できないので使わない。

## プロダクトの追加

```bash
ika project add <name> --path <リポジトリのパス> --summary "一行説明"
```

`data/projects.csv` に行が足され、`projects/<name>/README.md` がテンプレートから作られる。
