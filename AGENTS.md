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
| `data/projects.csv` | プロダクトの登録簿。プロダクト一覧の正本 |
| `data/domains.csv` | 領域の登録簿。`kb/` 直下のディレクトリ一覧の正本 |
| `.cache/fts/` | 全文検索（`ika kb fts`）の索引。git に入れない。Markdown からいつでも作り直せる |
| `data/datasets.sqlite3` | 表形式データ（価格の履歴・計測値・ログなど）。全データセットを1ファイルに持つ。人は直接読まず、`ika data` と `ika serve` で扱う |

### このリポジトリ（ツールとスキル）

| パス | 内容 |
|---|---|
| `docs/kb-format.md` | KB の書式の正本（フロントマター・置き場所・リンク） |
| `docs/data-format.md` | 登録簿など表形式データの書式 |
| `templates/` | 各種ノートのひな形（`ika` のビルド時にバイナリへ埋め込む） |
| `plugins/ika/` | ika プラグイン。Claude Code・claude.ai・Codex・ChatGPT で共通 |
| `plugins/ika/skills/<name>/` | スキル（[Agent Skills](https://agentskills.io/specification) 形式）。呼び出し名は `ika:<name>` |
| `.claude-plugin/marketplace.json` | Claude 向けプラグイン置き場の定義（このリポジトリ自体。公開ストアではなくローカル用） |
| `.agents/plugins/marketplace.json` | Codex・ChatGPT 向けプラグイン置き場の定義（ローカル用） |
| `cli/` | `ika` CLI（Rust）。登録・検証・各クライアント向けエクスポート |
| `dist/` | `ika export` の出力（Git管理外） |
| `local/` | Git に入れない下書き・機密メモ（Git管理外） |

詳しい書式は [`docs/kb-format.md`](docs/kb-format.md) と [`docs/data-format.md`](docs/data-format.md)。

## 守ること

- ナレッジの文章はテキスト（Markdown）で持ち、登録簿は CSV にする。バイナリやDBを正本にしない。例外は表形式データ（件数が増え、集計して使うもの）で、`data/datasets.sqlite3` に持つ。これは `ika data` 経由でだけ読み書きし、人が直接読まない
- 文章で書く知見・判断と、表で溜める観測を分ける。数値や記録が行として増え続けるもの（価格の履歴・計測値など）は `ika data` のデータセットに入れ、そこから言えることをノートや insight に書く。データセットは本文で名前を引用する
- Markdown は先頭にフロントマターを付ける（書式は `docs/kb-format.md`）。書いたら `ika check` で検証する
- ビジネス・ドメイン知識と技術は領域（`kb/business/` と `kb/tech/`）を分けて置く。迷ったら「何の判断に使うか」で決め、もう一方からリンクする（`docs/kb-format.md`）
- 「プロダクト」はコードのあるサービスに限らず、事業・副業の検討も含む（その場合 `path` は空）。プロダクト名は `data/projects.csv` の `name` と一致させる。新しいプロダクトは `ika project add` で登録する（手で増やすなら CSV の行と `kb/projects/<name>/README.md` を両方作る）
- 事実と意見を分ける。出典がある情報は `source:` にURLを書き、本文でも引用元を示す
- 日付は絶対日付（`2026-10-14`）で書く。「先週」「最近」は使わない
- ユーザーの判断が未確定のものは「案」「未決定」と明記し、決定事項として書かない
- 既存のノートがあれば新規作成より更新を優先する（特に `insights/` と `kb/projects/`）
- 各サービスのリポジトリ（`data/projects.csv` の `path`）は読むだけにし、このリポジトリから書き換えない
- KB は `ika` CLI で読み書きする。ファイルを直接 Read/Write/Edit/`rg` しない（正規表現などで探すなら `ika kb grep`）。`data/datasets.sqlite3` も `sqlite3` コマンドなどで直接触らない（外部プロジェクトの作業中も、このリポジトリの中でも同じ）。使うのは `ika new --stdin` / `ika kb cat|ls|search|write|append|edit|mv|rm`、登録簿は `ika project set`、表形式データは `ika data`。どのディレクトリからでも動くので、`Bash(ika:*)` の許可1つで権限確認が出ない。書き込みは自動でコミットされる

## 外部のドキュメントを取り込むとき

- `import` スキルで、製品リポジトリの仕様書・Google Drive・Notion・手元の docx・xlsx・pptx・pdf・html を `ika import` で取り込む。全文は `sources/` に写しとして置き、判断に使う要点は README・要件書・ノートに書き起こしてリンクする
- 取り込んだ資料は直接編集しない（取り込み直すと上書きされる）。`ika export` は既定で含めない（`--with-sources`）

## 調査から実装まで回すとき

- `cycle` スキルで、テーマ1つを「調査 → 要件（`kb/projects/<name>/specs/`）→ 実装 → テスト → コミット → KB の更新」の順に進める。要件の承認で1回だけ止まり、承認後はローカルの作業ブランチへのコミットまで進める（push・PR・デプロイはしない）
- 最後に README の「課題・次の打ち手」を更新し、それが次の回のテーマになる

## ノートを溜めたとき

- note は生のログ。溜めてよいが、溜まったら `ika kb orphans` で未整理を確認し、複数のノートから言えることは insight に書き直す（ノートのコピーはしない。手順は `capture` スキルの `references/distill.md`）
- 現状の設計・仕様・スキーマ（変わると古くなるもの）は note ではなく、`kb/projects/<name>/` の README か資料に書く

## ナレッジを探すとき

1. `ika project list` / `ika project show <name> --readme` で対象プロダクトの概要と関連ノートを確認する
2. 問いが技術かビジネスかで領域（`-d tech` / `-d business`）を絞り、`ika kb search` でキーワード・タグ（`-t`）・プロダクト（`-p`）を検索する。当たらなければもう一方の領域も探す
3. 回答には根拠にしたファイルのパスを添える。KBに無いことは「KBに記録なし」と言う

## スキルを足す・直すとき

- `plugins/ika/skills/<name>/SKILL.md` に書き、`name` はディレクトリ名と同じにする（プラグイン名で名前空間が付くので `ika-` は付けない）。`description` には「何をするか」と「どんな依頼で使うか」を書く
- 本文は短く保ち（500行未満）、詳しい資料は `references/`、ひな形は `assets/` に置いて相対パスでリンクする
- プラグインは自己完結させる（インストール時にプラグイン外へのシンボリックリンクは辿られない）。KB の書式とテンプレートの正本は `docs/kb-format.md` と `templates/` で、スキル内の `references/kb-format.md` と `assets/*.md` は `ika sync` が作るコピー。コピー側は直接編集しない
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

Rust のモジュールは `mod.rs` を使わず `foo.rs`（子モジュールがあれば `foo/`）で書く。

## CLI

```bash
cargo install --path cli        # ika をインストール（templates/ はバイナリに埋め込まれるので、ひな形を直したら入れ直す）
export IKA_HOME=<KB を置くディレクトリ>          # KB の場所。シェルの設定に足す（未設定だと KB 系コマンドは動かない）
ika init [--import <旧リポジトリ>]   # KB を作り、手元だけの git にする。--import は旧リポジトリの kb/・data/*.csv を取り込む
ika project list                # 登録済みプロダクト
ika project add <name> [--path <リポジトリのパス>] --summary "..."   # --path はコードのあるプロダクトだけ。事業・副業の検討なら省略
ika project show <name> [--readme]   # 登録情報と、projects: にそのプロダクトを含むノート・決定・知見の一覧
ika project set <name> [--url ...] [--summary ...] [--status ...] [--path ...]   # 登録済みの項目を書き換える
ika domain list                 # 登録済みの領域（business / tech）
ika domain add <name> --summary "..."
ika new note "タイトル" --domain tech --project <name> --tags <tag>,<tag> [--slug ...] [--stdin]  # --stdin: 標準入力を本文にする。日本語だけのタイトルは --slug（英字）が必須
ika new spec "タイトル" --project <name> --slug <英字> [--stdin]   # 要件書を kb/projects/<name>/specs/ に作る（--domain は付けない。status: draft で始まる）
ika import <file>... --project <name> | --domain <領域> [--project ...] [--tags ...] [--slug ...] [--title ...] [--source <URL>]   # 外部のドキュメント（md・html・csv・docx・xlsx・xls・ods・pptx・pdf）を Markdown にして sources/ に全文で置く。変換は Rust だけで行う。同じ source で取り込み直すと、変わっていたときだけ上書きする
ika import - --project <name> --source <URL> --title "..." [--format md|html|csv]   # 標準入力から（Drive・Notion の MCP で取った本文など）
ika import <file> --print       # KB に書かず、変換した Markdown を表示する
ika import --list [-p <name>]   # 取り込み済みの資料と原本の一覧
ika kb ls [kb/tech]             # kb/ 配下の Markdown を一覧（パスは IKA_HOME 下の kb/ から。kb/ の外は操作できない）
ika kb cat kb/tech/notes/2026/2026-10-14-foo.md   # 全文。--outline（見出し一覧と各節の行範囲・文字数）/ --section "見出し"（その節だけ。同名があれば "## 技術" と階層を付ける）/ --lines 40:80（行範囲。grep -n の行番号で）/ --frontmatter で一部だけ読める。同時に指定できるのは1つ
ika kb search review レビュー --any -d tech -p <name> -t <tag> -l   # 本文＋フロントマター（-p プロダクト -t タグ --type --status）で検索。--any は OR、-l でファイル名のみ。結果は信頼度順（採用済みの決定 → 知見 → プロダクト概要 → 案の決定 → ノート → 覆された決定）で、同順位では新しい日付が先
ika kb grep -n -C2 'review|レビュー' kb/tech   # ripgrep で検索する（正規表現・文脈行・-g・--json など。要 rg）。パスは IKA_HOME からの相対で kb/ の中だけ（省略すると kb/ 全体）。rg のオプションは許可リストにあるものだけ通し、--pre・-z・-f・-L など外部コマンドを動かす・kb/ の外を読むものは拒否する
ika kb fts 申請を取り下げたい [-d tech -p <name> -t <tag> --type ... --status ... --any -l -n 20]   # 日本語の形態素解析つきの全文検索（ika serve の検索画面が使う。エージェントの検索は kb search）。文でも引け、関連度順に並ぶ（上位20件。検索語なしは全件を新しい日付順）。語の途中でも当たる（2文字以上）。索引は IKA_HOME/.cache/fts/ にあり、検索のたびに変わったファイルだけ自動で入れ直す
ika kb index [--rebuild]        # fts の索引に kb/ の変更を反映する / 一から作り直す
ika kb orphans [-p <name>] [--min 3]   # どの insight・decision・プロダクトの資料（README を除く）からもリンクされていないノートをプロダクト別に数える（蒸留の候補）
ika kb write <path> [--overwrite]   # 標準入力の全文でファイルを作る
ika kb append <path>                # 標準入力を末尾に足す
ika kb edit <path> --old "..." --new "..." [--all]   # --old は1か所に一致する必要がある
ika kb mv <from> <to> [--fix-links]  # 移動・改名。移動先が存在すれば失敗。切れたリンクを表示し、--fix-links で書き換える
ika kb rm <path>                    # ファイルのみ。登録済みプロダクトの README は不可。リンク元を表示する
ika data new <name> --summary "..." --col date:date --col 'kind:enum(sold|listed)' --col price:int --col 'url:string?' --key date,kind [--domain ...] [--project ...]   # データセット（表）を作る。型は string/int/float/date/bool/enum(a|b)、`?` は省略可、--key は重複判定
ika data append <name> [--on-conflict error|skip|replace]   # 標準入力のヘッダ付き CSV を検証して追記（1行でも不正なら何も書かない）
ika data ls | show <name> [--tail 5]   # 一覧 / 定義・行数・末尾
ika data sql "select ..." [--json]  # 読み取り専用で SELECT（テーブル名はデータセット名）。集計・結合もここで
ika data export <name>              # CSV で標準出力へ
ika data delete <name> --where "..." / rm <name>   # 行の訂正は消して追記し直す / データセットごと消す
ika serve [--port 8787] [--open]   # KB をブラウザで読むローカルサーバー（読み取り専用。127.0.0.1 だけで待ち受け、Host は localhost・127.0.0.1 のみ）。毎回ファイルを読み直す。データセットも `/data/<name>` で読める（新しい順に最大200行）。タグ一覧は `/tags`、タグ別は `/tags/<タグ>`。検索画面は全文検索（`ika kb fts`）で、「文字列一致」にすると `ika kb search` と同じ探し方になる
ika check                       # KB・登録簿・データセット定義の検証。リポジトリ内で実行するとプラグインも検証する
ika sync                        # docs/kb-format.md・templates/ をプラグインのスキルにコピー（リポジトリ内で実行）
ika install [--bump patch|minor|major] [--dry-run]   # （リポジトリ内で実行）このリポジトリを手元のプラグイン置き場として Claude・Codex の設定に登録し、ika プラグインを入れる（入っていれば更新する。公開はしない）
ika export claude [--with-kb] [--domain <領域>]... [--no-projects] [--with-sources]   # dist/claude/ika.zip（Upload plugin 用）。--with-kb で KB のスナップショット（knowledge-*.md・CSV・instructions.md）も出す。クラウドに上がるので範囲は --domain・--no-projects で絞る
ika export chatgpt [--domain <領域>]... [--no-projects]   # dist/chatgpt/（ChatGPT web のプロジェクト用。IKA_HOME の KB を束ねる。リポジトリ内で実行）
```
