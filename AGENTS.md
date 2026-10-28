# ika — 個人ナレッジハーネス

開発中のサービスや事業・副業の検討、日々の情報収集・ビジネス知見を蓄積し、意思決定に使うためのリポジトリ。
Claude Code / Codex / Claude Desktop / ChatGPT から同じナレッジとスキルを使う。

このファイルが全クライアント共通の指示の正本。`CLAUDE.md` はこれを読み込むだけにする。

## ディレクトリ

### ナレッジのデータ（`IKA_HOME`。このリポジトリには置かない）

KB は変わり続けるデータなので、リポジトリにコミットせず、`IKA_HOME` のディレクトリに溜める。
`ika init` で作り、手元だけの git リポジトリとして `ika` の書き込みごとに自動コミットされる（リモートなし）。
`IKA_HOME` が未設定だと `ika` の KB 系コマンドは動かない（既定の場所は無い）。

| パス（`IKA_HOME` から） | 内容 |
|---|---|
| `kb/business/` | ビジネス・ドメイン知識（市場・顧客・業界・競合・価格・集客・法務） |
| `kb/tech/` | 技術（設計・実装・インフラ・ツール・運用） |
| `kb/<領域>/notes/YYYY/` | 日々の情報収集・調査メモ。日付つき、書いたら原則そのまま |
| `kb/<領域>/insights/` | 複数のメモから蒸留した知見。日付なし、育てて更新する |
| `kb/<領域>/decisions/` | 意思決定の記録（背景・選択肢・決定・理由） |
| `kb/projects/<name>/README.md` | プロダクトの概要（ビジネスと技術の両面）。1プロダクト1ディレクトリ |
| `kb/projects/<name>/specs/` | 要件書（`type: spec`。機能ごと。`cycle` スキルが作り、実装後に今の形へ直す） |
| `kb/projects/<name>/sources/`・`kb/<領域>/sources/` | 取り込んだ外部の資料の全文（`type: source`。`ika import` が作り、取り込み直すと上書きされる。直接編集しない） |
| `inbox/YYYY-MM-DD.md` | 受信箱。`ika capture` で判断なしに溜めた生のメモ（`## HH:MM` で区切って追記。フロントマターなし）。`kb/` の外なので検索・`ika check`・`export`・`serve` に出ない。整理してノートにしたら `ika inbox rm`。`ika inbox` だけで読み書きする |
| `data/projects.csv` | プロダクトの登録簿。プロダクト一覧の正本 |
| `data/domains.csv` | 領域の登録簿。`kb/` 直下のディレクトリ一覧の正本 |
| `.cache/fts/` | 全文検索（`ika kb fts`）の索引。git に入れない。Markdown からいつでも作り直せる |
| `.cache/html/` | `ika export html` の出力（KB と同じ階層の単独の HTML）。git に入れない。Markdown からいつでも作り直せる |
| `theme/style.css` | `ika export html` の見た目の上書き（任意。色・フォントの CSS 変数を `:root` で上書きする） |
| `data/datasets.sqlite3` | 表形式データ（価格の履歴・計測値・ログなど）。全データセットを1ファイルに持つ。人は直接読まず、`ika data` と `ika serve` で扱う |

<!-- repo-only -->

### このリポジトリ（ツールとスキル）

| パス | 内容 |
|---|---|
| `docs/kb-format.md` | KB の書式の正本（フロントマター・置き場所・リンク） |
| `docs/diagrams.md` | 図の記法（`seq`・`flow` など）の正本 |
| `docs/cli.md` | `ika` の全コマンドと主なオプション |
| `docs/data-format.md` | 登録簿など表形式データの書式 |
| `templates/` | 各種ノートのひな形（`ika` のビルド時にバイナリへ埋め込む） |
| `plugins/ika/` | ika プラグイン。Claude Code・claude.ai・Codex・ChatGPT で共通 |
| `plugins/ika/skills/<name>/` | スキル（[Agent Skills](https://agentskills.io/specification) 形式）。呼び出し名は `ika:<name>` |
| `.claude-plugin/marketplace.json` | Claude 向けプラグイン置き場の定義（このリポジトリ自体。公開ストアではなくローカル用） |
| `.agents/plugins/marketplace.json` | Codex・ChatGPT 向けプラグイン置き場の定義（ローカル用） |
| `cli/` | `ika` CLI（Rust）。登録・検証・各クライアント向けエクスポート |
| `crates/ika-diagram/` | 図の記法（`seq`・`flow` など）を解析し、CSS だけで描く HTML にするライブラリ（依存なし・publish しない） |
| `dist/` | `ika export` の出力（Git管理外） |
| `local/` | Git に入れない下書き・機密メモ（Git管理外） |

詳しい書式は [`docs/kb-format.md`](docs/kb-format.md) と [`docs/data-format.md`](docs/data-format.md)。
<!-- /repo-only -->

## 守ること

- ナレッジの文章はテキスト（Markdown）で持ち、登録簿は CSV にする。バイナリやDBを正本にしない。例外は表形式データ（件数が増え、集計して使うもの）で、`data/datasets.sqlite3` に持つ。これは `ika data` 経由でだけ読み書きし、人が直接読まない
- 文章で書く知見・判断と、表で溜める観測を分ける。数値や記録が行として増え続けるもの（価格の履歴・計測値など）は `ika data` のデータセットに入れ、そこから言えることをノートや insight に書く。データセットは本文で名前を引用する
- Markdown は先頭にフロントマターを付ける（書式は `docs/kb-format.md`）。書いたら `ika check` で検証する
- ビジネス・ドメイン知識と技術は領域（`kb/business/` と `kb/tech/`）を分けて置く。迷ったら「何の判断に使うか」で決め、もう一方からリンクする（`docs/kb-format.md`）
- 「プロダクト」はコードのあるサービスに限らず、事業・副業の検討も含む（その場合 `path` は空）。プロダクト名は `data/projects.csv` の `name` と一致させる。新しいプロダクトは `ika project add` で登録する（手で増やすなら CSV の行と `kb/projects/<name>/README.md` を両方作る）
- 事実と意見を分ける。出典がある情報は `source:` にURLを書き、本文でも引用元を示す（KB の中の話。製品のコード・ドキュメントでは一次情報の URL だけを書き、KB を出典にしない）
- 日付は絶対日付（`2026-10-14`）で書く。「先週」「最近」は使わない
- ユーザーの判断が未確定のものは「案」「未決定」と明記し、決定事項として書かない
- 既存のノートがあれば新規作成より更新を優先する（特に `insights/` と `kb/projects/`）
- 各サービスのリポジトリ（`data/projects.csv` の `path`）は読むだけにし、このリポジトリから書き換えない
- 製品リポジトリには ika・KB への言及を書かない（ika 自身のリポジトリを除く）。ika を使う人と使わない人が同じリポジトリで開発するため。コード・コメント・テスト名・ドキュメント・設定・コミットメッセージ・ブランチ名・PR に、ika・KB・`kb/` のパス・要件書・要件 ID を書かない。書き換え方と点検は `plugins/ika/skills/cycle/references/product-repo.md`
- KB は `ika` CLI で読み書きする。ファイルを直接 Read/Write/Edit/`rg` しない（正規表現などで探すなら `ika kb grep`）。`data/datasets.sqlite3` も `sqlite3` コマンドなどで直接触らない（外部プロジェクトの作業中も、このリポジトリの中でも同じ）。使うのは `ika new --stdin` / `ika kb cat|ls|search|write|append|edit|mv|rm`、登録簿は `ika project set`、表形式データは `ika data`。どのディレクトリからでも動くので、`Bash(ika:*)` の許可1つで権限確認が出ない。書き込みは自動でコミットされる

## 外部のドキュメントを取り込むとき

- `import` スキルで、製品リポジトリの仕様書・Google Drive・Notion・手元の docx・xlsx・pptx・pdf・html を `ika import` で取り込む。全文は `sources/` に写しとして置き、判断に使う要点は README・要件書・ノートに書き起こしてリンクする
- 取り込んだ資料は直接編集しない（取り込み直すと上書きされる）。`ika export` は既定で含めない（`--with-sources`）

## 調査から実装まで回すとき

- `cycle` スキルで、テーマ1つを「調査 → 要件（`kb/projects/<name>/specs/`）→ 実装 → テスト → コミット → KB の更新」の順に進める。要件の承認で1回だけ止まり、承認後はローカルの作業ブランチへのコミットまで進める（push・PR・デプロイはしない）
- 最後に README の「課題・次の打ち手」を更新し、それが次の回のテーマになる

## ノートを溜めたとき

- 領域などを決める前にとにかく残したいものは `ika capture "メモ"`（受信箱）。`ika kb triage` の先頭に出るので、`capture` スキルの `references/inbox.md` でノートに振り分ける
- note は生のログ。溜めてよいが、溜まったら `ika kb triage`（未整理と古いものを優先順位つきでまとめる。`ika kb orphans` は未整理だけ）で確認し、複数のノートから言えることは insight に書き直す（ノートのコピーはしない。手順は `capture` スキルの `references/distill.md`）
- 現状の設計・仕様・スキーマ（変わると古くなるもの）は note ではなく、`kb/projects/<name>/` の README か資料に書く

## ナレッジを探すとき

1. `ika project list` / `ika project show <name> --readme` で対象プロダクトの概要と関連ノートを確認する
2. 問いが技術かビジネスかで領域（`-d tech` / `-d business`）を絞り、`ika kb search` でキーワード・タグ（`-t`）・プロダクト（`-p`）を検索する。当たらなければもう一方の領域も探す
3. 回答には根拠にしたファイルのパスを添える。KBに無いことは「KBに記録なし」と言う

<!-- repo-only: ika export の指示から外す -->

## スキルを足す・直すとき

- `plugins/ika/skills/<name>/SKILL.md` に書き、`name` はディレクトリ名と同じにする（プラグイン名で名前空間が付くので `ika-` は付けない）。`description` には「何をするか」と「どんな依頼で使うか」を書く
- 本文は短く保ち（500行未満）、詳しい資料は `references/`、ひな形は `assets/` に置いて相対パスでリンクする
- プラグインは自己完結させる（インストール時にプラグイン外へのシンボリックリンクは辿られない）。KB の書式とテンプレートの正本は `docs/kb-format.md`・`docs/diagrams.md` と `templates/` で、スキル内の `references/kb-format.md`・`references/diagrams.md` と `assets/*.md` は `ika sync` が作るコピー。コピー側は直接編集しない
- マニフェストは2つある: `plugins/ika/.claude-plugin/plugin.json`（Claude）と `plugins/ika/plugin.json`（OpenAI）。`version` と `description` を揃える。プラグイン（スキル・references・assets・マニフェスト）を直したら、`ika install --bump patch` で version を上げて入れ直す（インストール済みのコピーは version が同じだと更新されない。`ika check` が上げ忘れを検出する）
- `ika check` と `claude plugin validate .` で検証する

## 開発

```bash
cargo build                     # CLI をビルド
cargo test                      # テスト
cargo clippy --all-targets      # lint（警告ゼロを保つ）
cargo fmt                       # 整形
claude plugin validate .        # プラグインとマーケットプレイスの公式検証
```

コマンドやオプションを変えたら [`docs/cli.md`](docs/cli.md) も直す（全サブコマンドが載っているかは `cargo test` が確かめる）。

Rust のモジュールは `mod.rs` を使わず `foo.rs`（子モジュールがあれば `foo/`）で書く。

## CLI

全コマンドは [`docs/cli.md`](docs/cli.md)、詳しくは `ika <コマンド> --help`。KB の作業はスキルの手順に従う。

- KB: `ika kb ls|cat|search|grep|fts|write|append|edit|mv|rm|orphans|stale|triage|review|index`、`ika new`（ノート・決定・要件書など）、`ika import`、`ika capture`・`ika inbox`
- 登録簿・データ: `ika project list|show|add|set`、`ika domain list|add`、`ika data new|append|ls|show|sql|export|delete|rm`
- 検証・閲覧・書き出し: `ika check`、`ika serve`、`ika export claude|chatgpt|html`
- セットアップ・開発: `cargo install --path cli`、`ika init`、`ika sync`、`ika install`
<!-- /repo-only -->
