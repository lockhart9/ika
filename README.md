# ika

開発中のサービス・日々の情報収集・ビジネス知見を蓄積し、意思決定に使うための個人用AIハーネス。
ナレッジとスキルはテキストファイルで持ち、Claude Code / Codex / Claude Desktop / ChatGPT から同じものを使う。

AIへの指示の正本は [`AGENTS.md`](AGENTS.md)、ナレッジの書式は [`docs/kb-format.md`](docs/kb-format.md)。

## 構成

このリポジトリはツールとスキルだけを持つ。ナレッジのデータ（KB）は変わり続けるので、リポジトリにはコミットせず `IKA_HOME` で指定したディレクトリに溜める。

```
このリポジトリ（Git・ツールとスキル）
  AGENTS.md          全クライアント共通の指示（CLAUDE.md はこれを読み込むだけ）
  docs/              KB・登録簿の書式の正本
  templates/         ノートのひな形（ika のビルド時にバイナリへ埋め込む）
  plugins/ika/       ika プラグイン（Claude・OpenAI 共通）
    .claude-plugin/plugin.json   Claude 用マニフェスト
    plugin.json                  OpenAI 用マニフェスト
    skills/<name>/               スキル。references/・assets/ は ika sync が正本からコピー
  .claude-plugin/marketplace.json  Claude 用プラグイン置き場の定義（ローカル用）
  .agents/plugins/marketplace.json Codex・ChatGPT 用プラグイン置き場の定義（ローカル用）
  cli/               ika CLI（Rust 2024 edition）

IKA_HOME（手元だけの git で履歴を残す。リモートなし）
  kb/
    business/        ビジネス・ドメイン知識（notes/YYYY・insights・decisions）
    tech/            技術（同じ構成）
    projects/<name>/ プロダクトごとの概要（両面）
  data/projects.csv  プロダクト登録簿（一覧の正本）
  data/domains.csv   領域登録簿（kb/ 直下の領域の正本）
  data/datasets.sqlite3  表形式データ（価格の履歴など。`ika data` で読み書き）
```

## セットアップ

```bash
cargo install --path cli     # ika コマンドを入れる
export IKA_HOME=<KB を置くディレクトリ>       # KB の場所。シェルの設定に足す。未設定だと ika の KB 系コマンドは動かない
ika init                     # KB を作る（手元だけの git リポジトリ。以後、書き込みごとに自動コミット）
ika check                    # KB を検証
```

旧構成（KB をこのリポジトリの `kb/`・`data/` に置いていた）からの移行は `ika init --import <旧リポジトリ>`。

スキルは ika プラグインとして入れる。どのリポジトリで作業していても使える:

```bash
ika install --dry-run        # 実行するコマンドを確認
ika install                  # claude plugin marketplace add <このリポジトリ> → claude plugin install ika@ika、
                             # codex があれば codex plugin marketplace add <このリポジトリ>
```

`ika install` などリポジトリを扱うコマンドは、リポジトリの中で実行する。

`ika install` は、このディレクトリのパスを各クライアントの設定に登録するだけで、外部へ公開・送信はしない（Claude・Codex は、プラグインを探す一覧を「マーケットプレイス」と呼ぶ。ここではローカルのディレクトリ）。
### スキルを直したとき

Claude Code・Codex・ChatGPT デスクトップは、インストール時にプラグインを**キャッシュへコピー**して使う（Claude Code は `~/.claude/plugins/cache/`）。リポジトリを直しても、コピーは自動では変わらず、**`version` が同じだと更新もされない**。直したら次を実行する:

```bash
ika install --bump patch     # 2つのマニフェストの version を上げ、Claude Code のプラグインを更新する
                             # （入っていなければ install、入っていれば plugin update）
```

更新の反映には Claude Code の再起動が必要。`ika install` は最後に、インストール済みのコピーがリポジトリと同じ内容になったかを確かめ、古いままなら失敗で終わる。
`ika check` は、`plugins/ika` の中身が変わっているのに `version` が最後に上げたときのままなら、エラーにする（未コミットの編集と新規ファイルも見る）。インストール済みのコピーが古いときは、一言添える。
Codex・ChatGPT デスクトップは、`ika install` が marketplace を登録するだけなので、`version` を上げたあとに Plugins 画面で再読み込みする。

## クライアントごとの使い方

| クライアント | 指示 | スキル | ナレッジの読み書き |
|---|---|---|---|
| Claude Code | `CLAUDE.md`（`@AGENTS.md` を読み込む） | `ika install` で ika プラグインを入れる | `ika` CLI（`Bash(ika:*)` を許可） |
| Codex | `AGENTS.md` | `ika install` でマーケットプレイスを登録し、ika プラグインを有効にする | `ika` CLI |
| ChatGPT デスクトップ | このリポジトリをローカルプロジェクトの primary フォルダにすると `AGENTS.md` を読む | Plugins 画面で ika を有効にする（`.agents/plugins/marketplace.json` から見つかる） | `ika` CLI |
| Claude Desktop | プロジェクトの指示に `AGENTS.md` を貼る | `ika install` で ika プラグインを入れる（Code タブ）。チャットだけなら `ika export claude` の zip を **Customize > Plugins > Add > Upload plugin** で上げる（Claude Code にも同期されるので、`ika install` と両方入れると重複する） | Code タブは `ika` CLI。チャットはファイルシステム MCP サーバー（`@modelcontextprotocol/server-filesystem`）で `IKA_HOME` のディレクトリだけを許可する（`ika` CLI は呼べないので、自動コミットや検証は効かない。次に `ika` で書いたときのコミットに入る） |
| claude.ai（ブラウザ） | `ika export claude --with-kb` の `instructions.md` をプロジェクトの指示に | `ika export claude` の zip を上記の Upload plugin で上げる | **既定では KB に触れない**（ローカルのファイルにも CLI にも届かない）。`ika export claude --with-kb` で `knowledge-*.md` と CSV を作り、プロジェクトの知識に上げれば**読み取り専用**で参照できる。書き出した時点のスナップショットで、KB の内容がクラウドに上がる。記録は出力された本文を手で保存 |
| ChatGPT web | `ika export chatgpt` の `instructions.md` をプロジェクトの指示に | `skills.md` をプロジェクトファイルに | `knowledge-*.md` と CSV をプロジェクトファイルに上げる。領域ごとに分かれているので、技術だけ・ビジネスだけのプロジェクトも作れる（読み取り専用。記録は出力された本文を手で保存） |

`dist/` は書き出し専用で Git 管理外。KB を更新したら書き出し直す（KB は `IKA_HOME` から読む）。

KB を書き出すとき（`ika export chatgpt`、`ika export claude --with-kb`）は、クラウドに上がる範囲を絞れる。

```bash
ika export claude --with-kb --domain tech --no-projects   # 技術の領域だけ。プロダクト概要と projects.csv は含めない
```

`--domain` は複数指定できる。書き出すたびに前回の `knowledge-*.md` と CSV は消えるので、範囲を絞り直しても古い内容は残らない。`--with-kb` を付けない `ika export claude` は、プラグインの zip だけを作る（KB は含まない）。

## ブラウザで読む

```bash
ika serve --open             # http://127.0.0.1:8787/ を開く（--port で変更。0 なら空きポート）
```

`ika serve` は、KB を Markdown のままブラウザで読むための読み取り専用のサーバー。

- トップ（プロダクト・領域・最近の更新）、領域・年ごとのディレクトリ一覧、ノートの表示（フロントマターはタイトル・種類・日付・プロダクト・タグ・出典として表示し、本文の Markdown を描画）、プロダクトのページ（README と関連するノート・決定・知見）、検索（信頼度順。領域・プロダクト・種類・タグで絞れる）
- ノート同士の相対リンクはそのまま辿れ、各ページの下に「このページへのリンク」が出る
- ファイルは毎回読み直すので、`ika` で書いたあとは再読み込みで見える
- `127.0.0.1` だけで待ち受け、GET・HEAD しか受け付けない。Host が `localhost`・`127.0.0.1` 以外のリクエストは断る（DNS リバインディング対策）。KB の外・シンボリックリンク経由は読めず、返すのは Markdown と画像（png・jpg・gif・webp）だけ。ノートに書かれた生の HTML は実行せずそのまま文字として表示し、`javascript:` のリンクは無効にする

## プロダクトを増やす

```bash
ika project add <name> --path <リポジトリのパス> --url https://... --summary "一行説明"
```

`data/projects.csv` に1行足され、`kb/projects/<name>/README.md` が作られる（どちらも `IKA_HOME` の下）。中身は `ika:project-sync` スキルでリポジトリから埋める。

## 領域を増やす

ビジネス・ドメイン知識（`business`）と技術（`tech`）は最初から分けてある。別の切り口が必要になったら:

```bash
ika domain add <name> --summary "何を置く領域か"
```

## スキル

呼び出し名はプラグイン名付きの `ika:<name>`（Claude Code では `/ika:capture` など）。

| スキル | 用途 |
|---|---|
| `capture` | 情報・知見を記録する |
| `search` | KB を検索して根拠付きで答える |
| `decide` | 過去の経緯を踏まえて論点を整理し、決定を記録する |
| `project-add` | プロダクトを登録する |
| `project-sync` | リポジトリを読んでプロダクト概要を更新する |

`docs/kb-format.md` や `templates/` を直したら `ika sync` でスキル側のコピーを更新する（`templates/` はバイナリにも埋め込まれるので `cargo install --path cli` も）（`ika check` が差分を検出する）。

## 今後の候補

- `ika search`: KB が増えて `rg` で足りなくなったら、SQLite（FTS5）の検索インデックスを作る。正本はテキストのまま、インデックスは再生成できるものにする
- `ika mcp`: KB の検索・記録を MCP サーバー（stdio）として出し、Claude Desktop と Codex から同じツールで使う
- 指標データ（`data/metrics/*.csv` など）とプロダクト横断のレポート
