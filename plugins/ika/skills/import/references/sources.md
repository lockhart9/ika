# 原本の取り方

`ika import` に渡すのは、ファイルのパスか、標準入力の本文。原本のある場所ごとの取り方。

## 製品リポジトリ・手元のファイル

パスをそのまま渡す。原本（`source:`）はファイルの絶対パスになり、同じパスで取り込み直せる。

```bash
ika import ~/work/<repo>/docs/*.md --project <name> --tags spec
ika import ~/Downloads/見積書.xlsx --project <name> --slug estimate
```

- 製品リポジトリは読むだけにする。リポジトリの Markdown は、取り込んでも原本は書き換わらない
- 1つのコマンドで複数のファイルを渡せる。そのときは `--source`・`--title`・`--slug` を使えない（ファイルごとに決まる）

## Google Drive

Google Drive の MCP コネクタで探して取る。つながっていなければ、ユーザーにコネクタの接続を頼む（claude.ai・Claude Desktop は設定のコネクタ、Claude Code は `/mcp`）。

- Google ドキュメント・スプレッドシート・スライド: コネクタが返す本文（テキスト・Markdown・CSV）を標準入力で渡す。表なら `--format csv`、HTML で返るなら `--format html`
- アップロードされた docx・xlsx・pptx・pdf: ファイルとして取れるならスクラッチのディレクトリに保存してパスで渡し、`--source` に Drive の URL を付ける。本文しか取れないなら標準入力で渡す
- `--source` は Drive の URL（`https://docs.google.com/document/d/<id>/...`、`https://drive.google.com/file/d/<id>/...`）。取り込み直すときも同じ URL を使う

```bash
ika import ./scratch/pricing.xlsx --project <name> --source https://drive.google.com/file/d/<id>/view --slug pricing
ika import - --project <name> --source https://docs.google.com/document/d/<id>/edit --title "要件定義書" --slug requirements <<'EOF'
...コネクタが返した本文...
EOF
```

## Notion

Notion の MCP（ページを Markdown で返す）で取り、標準入力で渡す。`--source` はページの URL。

つながっていなければ、ユーザーに接続を頼む。

- Claude Code: `claude mcp add --transport http notion https://mcp.notion.com/mcp` を実行し、`/mcp` で認証する
- claude.ai・Claude Desktop: 設定のコネクタから Notion を追加する
- Codex など MCP をつなげないとき: Notion の「エクスポート → Markdown & CSV」の zip を展開し、`.md` をパスで渡す。原本の目印にはページの URL を `--source` で付ける（1ファイルずつ）

子ページは別のページとして1つずつ取り込む。データベースは、表として CSV で渡す（`--format csv`）か、行が増え続けるなら `ika data` のデータセットに入れる。

## Web ページ

- 保存した HTML はパスで渡す（`--source` に元の URL を付ける）
- Web から読んだ本文が Markdown で手元にあるなら、標準入力で渡す（`--source` に URL）
- 記事の全文の転載は、自分用の KB でも必要な範囲に留める。要点だけでよければ取り込まず、`capture` スキルでノートにする
