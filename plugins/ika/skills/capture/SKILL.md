---
name: capture
description: 情報収集の結果・調査メモ・ビジネスやドメインの知識・技術的な知見を ika のナレッジベースに記録する。「メモして」「KBに残して」「記録しておいて」「この記事をまとめて保存」のような依頼や、調査を終えて残す価値のある事実が出たときに使う。受信箱（`ika capture` で判断なしに溜めたメモ）を片付けたいとき（「受信箱を整理して」）にも使う。
compatibility: KB（環境変数 IKA_HOME のディレクトリ）の読み書きには ika CLI が必要。Claude Code・Codex・ChatGPT デスクトップアプリ向け
---

# capture: ナレッジを記録する

KB は ika のリポジトリではなく環境変数 `IKA_HOME` のディレクトリにある。未設定だと `ika` は動かないので、設定を頼む。読み書きは `ika` CLI で行い、ファイルを直接 Read/Write/rg しない。構成・フロントマター・領域の振り分けは [references/kb-format.md](references/kb-format.md)、ひな形は [assets/note.md](assets/note.md) と [assets/insight.md](assets/insight.md)。
製品リポジトリには ika・KB への言及を書かない（ika 自身のリポジトリを除く）。コード・コメント・テスト名・ドキュメント・設定・コミットメッセージ・PR に、ika・KB・`kb/` のパス・要件書を書かず、出典は一次情報の URL だけにする。詳しくは [cycle の資料](../cycle/references/product-repo.md)。
KB にアクセスできない環境（claude.ai・ChatGPT web など）では、保存先のパスとフロントマター付きの本文を出力し、ユーザーに保存してもらう。

## 手順

1. 領域を決める（`ika domain list`）。基準は「何の判断に使うか」
   - 何を作るか・誰に届けるか・どう稼ぐか（市場・顧客・業界・競合・価格・集客・法務）→ `business`
   - どう作るか・どう動かすか（設計・実装・インフラ・ツール・運用・技術的なコスト）→ `tech`
   - 両方にまたがるなら主に使う側に置き、もう一方の関連ノートから相対リンクを張る
2. 種類を決める（何を書くかで決まる）
   - 起きたこと・調べたこと（その時点のログ）→ `note`
   - **今の状態**（設計・仕様・スキーマ・ドメインモデルなど、変わると古くなるもの）→ note にしない。`kb/projects/<name>/README.md` を更新するか、その下に資料を作る（`ika kb write kb/projects/<name>/<topic>.md`。type は `note` でよい。日付は最終更新日にして、更新して育てる）
   - 複数の事実から言える一般化された知見 → `insight`。先に `ika kb search --type insight -p <name> -l` で既存を探し、あれば更新する。書き方は [references/distill.md](references/distill.md)（ノートのコピーではなく、書き直して一文に絞る）
   - 何かを決めた・決めたい → `decide` スキルに任せる
   - **同じ形の行が増え続け、集計して使う観測**（価格の履歴・計測値・ログなど）→ ノートに書かずデータセットに溜める（`ika data`）。手順は [references/data.md](references/data.md)。そこから言えることを note / insight に書き、データセット名を本文で引用する
3. 関連プロダクトを `ika project list` の `name` から選ぶ。該当がなければ `projects: []`。未登録の新プロダクトなら `project-add` スキルを先に使う
4. ファイルを作る。CLI があれば使う（どのディレクトリからでも動く。KB のファイルを直接 Write/Edit しない）:
   ```bash
   ika new note "タイトル" --domain tech --project <name> --tags <tag>,<tag> --slug <英字> --stdin <<'EOF'
   ## 要点
   ...本文...
   EOF
   ```
   `--slug` はファイル名になる英字（日本語だけのタイトルでは必須。内容が分かる名前にする）。`--stdin` を付けるとテンプレートの本文の代わりに標準入力が本文になる（フロントマターは引数から作られる）。既存ノートの更新は `ika kb edit <path> --old ... --new ...` か `ika kb append <path>`。ほかに `ika kb cat|ls|search|write|mv|rm`（パスは `kb/` から）。書き込みは原子的で、並行セッションが同時に変えたファイルは上書きせず失敗するので、読み直してやり直す。`mv`・`rm` は、リンク切れになる参照元を表示する（`mv --fix-links` なら自動で書き換える。`rm` は `ika kb edit` で直す）。
   作った直後に標準エラーへ「似た既存のナレッジ」が出たら、新規で足りるか確かめる。同じ話題なら既存のものに `ika kb append` / `edit` で足し、作ったファイルは `ika kb rm` で消してよい。関連するなら本文からリンクを張る。「README の『関連』に足す候補」が出ていれば、その README の「関連」に足す。何も出なければ似たものは無い（「調査」「まとめ」のような一般的な語だけのタイトルでは出ない）
   CLI が無ければ `assets/` のひな形をコピーし、[references/kb-format.md](references/kb-format.md) のパス規則で作る
5. 本文を書く（`--stdin` で渡していればここまでで済んでいる）
   - 冒頭の「要点」は3行以内で、後から読んだ自分が判断に使える形にする
   - 出典URLは `source:` と本文の両方に書く。引用と自分の解釈を分ける（KB のノートの話。製品のコード・ドキュメントでは一次情報の URL だけを書き、KB を出典にしない）
   - 日付は絶対日付。未確認のことは「未確認」と書く
   - 流れ・構成・データの形・経緯・案の比較は図にすると速く伝わる。[references/diagrams.md](references/diagrams.md) の記法（` ```seq `・` ```flow ` など）で書く。Mermaid は使わない
6. `ika kb` は書き込み後に指摘（フロントマター・リンク切れ・図の記法の誤り）を表示するので直す。最後に `ika check` を通す。「注意」に今書いたノート（どこからもリンクされていない）が出ていれば、insight・決定・README の「関連」からリンクを張る（CLI が無ければフロントマターを目視で確認）
7. 関連する `kb/projects/<name>/README.md` の「関連」（ビジネス／技術）や `insights/` に影響があれば追記を提案する
8. note を書いたら `ika kb orphans -p <name>` を実行する。「要整理」（3件以上）が出ていれば、[references/distill.md](references/distill.md) の手順で insight への蒸留をユーザーに提案する（押し付けず、断られたらそのまま）。続けて `ika kb stale -p <name>` を実行し、今書いたノートより前の知見が「新しいノートを取り込んでいない知見」に出ていれば、その知見の更新も同じ手順で提案する（`unrecognized subcommand` と出る古い ika では飛ばす）

## 受信箱

領域・プロダクト・タグを決める前に、まず残したいだけのときは `ika capture "メモ"`（URL でもよい。複数行は `--stdin`）で受信箱に溜められる。判断はいらない。溜まったメモの振り分けは [references/inbox.md](references/inbox.md) の手順で行う（`ika inbox ls` で読み、ノートにしたら `ika inbox rm <日付>`）。`unrecognized subcommand` と出る古い ika では使えない。

## しないこと

- 行として増え続ける観測を、note の表に書き足し続けない（`ika data` に入れる）
- ユーザーが言っていない判断・評価を、決定事項として書かない
- 長い記事を全文転載しない。要点と自分たちへの示唆に絞る
- 現状の設計・スキーマを note に書いて放置しない（変わっても古い内容が正しいまま残る）
