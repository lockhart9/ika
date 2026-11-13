---
name: import
description: 外部の資料を Markdown にして ika のナレッジベースに全文で取り込み、要点を README・要件書・ノートに反映する。取り込まずに参照先として登録もする。対象は製品リポジトリの仕様書、Google Drive・Notion、docx・xlsx・pptx・pdf・html・csv。「この仕様書を取り込んで」で使う。
compatibility: ika CLI が必要（変換は ika import が行い、外部ツールは要らない）。Google Drive・Notion から取るには、それぞれの MCP コネクタが要る
---

# import: 外部のドキュメントを取り込む

外部のドキュメントを `ika import` で Markdown にし、全文を KB に置く。原本が Drive や Notion にあっても、どのクライアント（Codex・ChatGPT を含む）からも同じ内容を読めるようにするため。全文は原本の写しで、判断に使う要点は README・要件書・ノートに書き起こしてリンクする。

KB は環境変数 `IKA_HOME` のディレクトリにある（未設定なら設定を頼む）。読み書きは `ika` CLI だけで行い、ファイルを直接 Read/Write/rg しない。原本の取り方は [references/sources.md](references/sources.md)。
製品リポジトリには ika・KB への言及を書かない（ika 自身のリポジトリを除く。出典は一次情報の URL だけ。詳しくは [cycle の資料](../cycle/references/product-repo.md)）。

取り込んだ資料は `type: source` で、次のどちらかに置かれる。

- `kb/projects/<name>/sources/`: プロダクトの資料（仕様書・設計書・企画書）。`--project <name>`
- `kb/<領域>/sources/`: 特定のプロダクトに属さない資料（市場レポート・業界資料・制度の解説）。`--domain business` など。`--project` も付けると関連プロダクトになる

フロントマターの `source:` に原本の URL かパスが入る。同じ `source` でもう一度取り込むと、中身が変わっていたときだけ上書きされる。

## 手順

1. 置き場所を決める（`ika project list`・`ika domain list`）。迷ったら「何の判断に使うか」で決める（迷ったときだけ [references/kb-layout.md](references/kb-layout.md) を読む）
2. 原本を手に入れて取り込む。取り方は [references/sources.md](references/sources.md)
   ```bash
   ika import docs/spec.md docs/api.md --project <name> --tags spec    # ファイル（複数可。原本はファイルの絶対パス）
   ika import ~/Downloads/report.pdf --domain business --project <name> --slug market-report-2026
   ika import - --project <name> --source <URL> --title "タイトル" --slug <英字> <<'EOF'
   ...MCP で取ってきた本文（Markdown）...
   EOF
   ika import report.docx --print | head -50   # KB に書かず、変換の結果だけを見る
   ```
   - 形式は拡張子で決まる: md・txt・html・csv・docx・xlsx（xls・ods）・pptx・pdf。標準入力は既定で md（`--format html` などで変えられる）
   - タイトルは、文書のプロパティ（docx・pptx・pdf）・最初の `#` 見出し・ファイル名の順に決まる。`--title` で指定もできる
   - KB 上のファイル名は、原本のファイル名から作る。同じ名前がすでにあれば「フォルダ名-ファイル名」にする（`docs/<機能>/requirements.md` が並んでいても重ならない）。原本のファイル名が英字にならないときと標準入力ではタイトルから作るので、どちらも英字にならなければ `--slug` を付ける
   - Drive・Notion の原本は、必ず `--source` に原本の URL を付ける（取り込み直すときの目印になる）
3. 結果を確かめる
   - 表示された「注意」を読む。画像は取り込まれない。行数の多いシートは `ika data` のデータセットに入れるか検討する（[capture のデータ手順](../capture/references/data.md)）
   - PDF から文字を取れないとき（スキャン画像など）は、AI が PDF を読んで Markdown に起こし、`ika import - --format md --source <原本> --title ...` で渡す
   - 長い資料は `ika kb cat <path> --outline` で構成を見る
4. 要点を書き起こす（これをやらないと、全文が置かれただけで判断に使われない）
   - 仕様書・設計書 → `kb/projects/<name>/README.md` の該当する節か、その下の資料に「今の状態」として書く。これから作る機能なら、`cycle` スキルの要件書にする
   - 市場・競合・制度の資料 → `capture` スキルで `business` のノートにする（要点と自分たちへの示唆。取り込んだ資料へ相対リンクを張る）
   - 決定事項が書かれていれば、`decide` スキルで決定として記録する（ユーザーが決めたことか確かめる）
   - README の「関連」に、取り込んだ資料へのリンクを足す
   - 全文をノートにコピーしない。引用は必要な数行だけにする
5. `ika check` を通す

## 取り込まずに参照先として登録する

会社の Drive のフォルダ、共有のスプレッドシート、別の SaaS、手元のフォルダのように、写しきれない・変わり続ける・権限の要る場所は、取り込まずに外部ナレッジの目録に登録する。`search`・`brainstorm`・`cycle` の調査が KB の後に目録を見て、必要なときだけその場で引く。「〜を参照先に登録して」「会社の Drive も見るようにして」と頼まれたときもこの手順。

| 写す（`ika import`） | 登録する（`ika external add`） |
|---|---|
| 判断の根拠として、その時点の版を残したい | 量が多い（フォルダごと・ワークスペースごと） |
| 量が少ない・ほとんど変わらない | 変わり続ける（共有のスプレッドシート・議事録） |
| コネクタの無いクライアントでも全文を読みたい | 権限が要る・社外秘で KB に写したくない |

```bash
ika external add work-drive-specs --kind connector --connector "Google Drive" \
  --location https://drive.google.com/drive/folders/<id> -p <name> -d business \
  --about "営業資料と価格表（スプレッドシート）。価格・顧客の問いで見る"
ika external add reading-notes --kind folder --location ~/Documents/notes --about "読書メモ。技術書の要点を探すときに見る"
ika external add vendor-docs --kind web --location https://docs.example.com --about "使っている API の公式ドキュメント"
ika external list [-p <name>] [-d <領域>]    # 登録の一覧（読み方つき）
ika external rm <name>                       # 目録から外す（外部の中身には触れない）
```

- `--kind`: `connector`（クライアントのコネクタ・MCP で読む。`--connector` に、クライアントでの表示名を書く）／`folder`（手元のフォルダ。絶対パスか `~`）／`web`
- `--about` は、何が入っていて、どんな問いのときに見るかを書く。エージェントが見るかどうかをこれで決める。Drive のコネクタは、アップロードした xlsx・pdf を検索で返さないことがあるので、中身（どのスプレッドシートに何があるか）も書いておく
- `-p`・`-d` を省くと、すべてのプロダクト・領域に関係する扱いになる
- 目録は `IKA_HOME` ごとにある。仕事と個人で見る先が違うなら `IKA_HOME` を分ける
- 資格情報（パスワード・API キー・トークン）は登録しない。つなぐのはクライアントのコネクタの設定で行う

## 取り込み直す

「資料を最新にして」「取り込み直して」と頼まれたら、次の順に進める。

1. `ika import --list [-p <name>]` で取り込み済みの資料と原本を一覧する
2. 原本ごとに、最初と同じ方法で取り込む（ファイルはそのパスで、Drive・Notion は取り直して同じ `--source` で）
3. 出力を見る: `変更なし` か `取り込み直しました（+N -M 行）`
4. 変わった資料について、要点を書き起こした先（`ika kb grep -l '<ファイル名>'` でリンク元を探す）が古くなっていないか確かめて直す。違いを報告する

## しないこと

- 取り込んだ資料を `ika kb edit` で直さない（取り込み直すと上書きされる）。直したい内容は README・要件書・ノートに書く
- Drive・Notion・製品リポジトリの原本を書き換えない（読むだけ）
- パスワード・鍵・個人情報が入った資料は、取り込む前にユーザーに確かめる
- `ika export` は既定で取り込んだ資料を含めない。クラウドに上げてよいときだけ `--with-sources` を付ける
