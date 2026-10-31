# ika CLI

`ika` の全コマンドと主なオプション。各コマンドの詳しいオプションは `ika <コマンド> --help`。コマンドやオプションを変えたら、ここも直す（全サブコマンドが載っているかは `cargo test` が確かめる）。

```bash
cargo install --path cli        # ika をインストール（templates/ はバイナリに埋め込まれるので、ひな形を直したら入れ直す）
export IKA_HOME=<KB を置くディレクトリ>          # KB の場所。シェルの設定に足す（未設定だと KB 系コマンドは動かない）
ika init [--import <旧リポジトリ>]   # KB を作り、手元だけの git にする。--import は旧リポジトリの kb/・data/*.csv を取り込む
ika project list                # 登録済みプロダクト
ika project add <name> [--path <リポジトリのパス>] --summary "..."   # --path はコードのあるプロダクトだけ。事業・副業の検討なら省略
ika project show <name> [--readme] [-n 20]   # 登録情報と、projects: にそのプロダクトを含むノート・決定・知見の一覧（既定は20件まで。-n 0 で全件。--readme は全文）
ika project set <name> [--url ...] [--summary ...] [--status ...] [--path ...]   # 登録済みの項目を書き換える
ika domain list                 # 登録済みの領域（business / tech）
ika domain add <name> --summary "..."
ika new note "タイトル" --domain tech --project <name> --tags <tag>,<tag> [--slug ...] [--stdin]  # --stdin: 標準入力を本文にする。日本語だけのタイトルは --slug（英字）が必須。作った直後に標準エラーへ、似た既存のナレッジ（上位3件。タイトルとタグの語から fts で探す）と、リンクを足す README の候補を出す（標準出力は作ったパスだけ。`ika kb write` の新規作成も同じ）
ika new spec "タイトル" --project <name> --slug <英字> [--stdin]   # 要件書を kb/projects/<name>/specs/ に作る（--domain は付けない。status: draft で始まる）
ika import <file>... --project <name> | --domain <領域> [--project ...] [--tags ...] [--slug ...] [--title ...] [--source <URL>]   # 外部のドキュメント（md・html・csv・docx・xlsx・xls・ods・pptx・pdf）を Markdown にして sources/ に全文で置く。変換は Rust だけで行う。同じ source で取り込み直すと、変わっていたときだけ上書きする
ika import - --project <name> --source <URL> --title "..." [--format md|html|csv]   # 標準入力から（Drive・Notion の MCP で取った本文など）
ika import <file> --print       # KB に書かず、変換した Markdown を表示する
ika import --list [-p <name>]   # 取り込み済みの資料と原本の一覧
ika kb ls [kb/tech]             # kb/ 配下の Markdown を一覧（パスは IKA_HOME 下の kb/ から。kb/ の外は操作できない）
ika kb cat kb/tech/notes/2026/2026-10-14-foo.md   # 全文。--outline（見出し一覧と各節の行範囲・文字数）/ --section "見出し"（その節だけ。同名があれば "## 技術" と階層を付ける）/ --lines 40:80（行範囲。grep -n の行番号で）/ --frontmatter で一部だけ読める。同時に指定できるのは1つ。8000字を超えるファイルは指定がなければ見出し一覧だけを出す（全文は --full。標準エラーに案内が出る）
ika kb search review レビュー --any -d tech -p <name> -t <tag> -l   # 本文＋フロントマター（-p プロダクト -t タグ --type --status）で検索。--any は OR、-l でファイル名のみ。結果は信頼度順（採用済みの決定 → 知見 → プロダクト概要 → 案の決定 → ノート → 覆された決定）で、同順位では、含む検索語の種類が多い順 → 一致行が多い順 → 新しい日付の順。出力は既定で20ファイル（-l と検索語なしは50）・1ファイル3行・1行160字までで、超えると標準エラーに件数を出す（-n で増やす、-n 0 で無制限。語が一般的だと信頼度順のまま後ろが切れるので、-p -d --type で絞る）
ika kb grep -n -C2 'review|レビュー' kb/tech   # ripgrep で検索する（正規表現・文脈行・-g・--json など。要 rg）。パスは IKA_HOME からの相対で kb/ の中だけ（省略すると kb/ 全体）。rg のオプションは許可リストにあるものだけ通し、--pre・-z・-f・-L など外部コマンドを動かす・kb/ の外を読むものは拒否する
ika kb fts 申請を取り下げたい [-d tech -p <name> -t <tag> --type ... --status ... --any -l -n 20]   # 日本語の形態素解析つきの全文検索（ika serve の検索画面が使う。エージェントの検索は kb search）。文でも引け、関連度順に並ぶ（上位20件。検索語なしは全件を新しい日付順）。語の途中でも当たる（2文字以上）。索引は IKA_HOME/.cache/fts/ にあり、検索のたびに変わったファイルだけ自動で入れ直す
ika kb index [--rebuild]        # fts の索引に kb/ の変更を反映する / 一から作り直す
ika kb orphans [-p <name>] [--min 3]   # どの insight・decision・プロダクトの資料（README を除く）からもリンクされていないノートをプロダクト別に数える（蒸留の候補）
ika kb stale [-p <name>] [--days 90] [--volatile pricing,competitor,...]   # 古くなっていそうなナレッジを洗い出す: リポジトリより古い README（git log を読むだけ）、その後のノートを取り込んでいない知見、古い価格・競合などのノートを根拠にした知見・決定・要件書・README、原本が変わった・消えた取り込み資料（手元のファイルだけ）。項目ごとに直し方を出す
ika capture "メモ" | ika capture --stdin   # 領域・プロダクト・タグを決めずに IKA_HOME/inbox/YYYY-MM-DD.md へ即保存（URL も文字のまま。取得はしない）。書き込みは自動でコミットされる
ika inbox ls [--date YYYY-MM-DD]   # 受信箱を全文で読む
ika inbox rm <YYYY-MM-DD>       # 整理し終えた日を消す
ika kb review <path>... [--date YYYY-MM-DD]   # 確かめ直したナレッジに確かめた日（フロントマターの reviewed）を付ける・更新する。内容を確かめてから実行する。stale・triage は date と reviewed の遅いほうを基準にする。取り込み資料（source）には付けられず、1つでも付けられなければ何も書かない。書き込みは自動でコミットされる
ika kb triage [-p <name>] [--limit 10] [--min 3] [--days 90] [--volatile ...]   # orphans と stale をまとめ、優先順位つき（受信箱 → 蒸留 → 知見の更新 → README → 根拠の確かめ直し → 取り込み直し。同種は件数順）の「今週やること」にする。読むだけで何も書かない。未整理が --min 件未満のプロダクトは「様子見」
ika kb write <path> [--overwrite]   # 標準入力の全文でファイルを作る
ika kb append <path>                # 標準入力を末尾に足す
ika kb edit <path> --old "..." --new "..." [--all]   # --old は1か所に一致する必要がある
ika kb mv <from> <to> [--fix-links]  # 移動・改名。移動先が存在すれば失敗。切れたリンクを表示し、--fix-links で書き換える
ika kb rm <path>                    # ファイルのみ。登録済みプロダクトの README は不可。リンク元を表示する
ika data new <name> --summary "..." --col date:date --col 'kind:enum(sold|listed)' --col price:int --col 'url:string?' --key date,kind [--domain ...] [--project ...]   # データセット（表）を作る。型は string/int/float/date/bool/enum(a|b)、`?` は省略可、--key は重複判定
ika data append <name> [--on-conflict error|skip|replace]   # 標準入力のヘッダ付き CSV を検証して追記（1行でも不正なら何も書かない）
ika data ls                         # データセットの一覧
ika data show <name> [--tail 5]     # 定義・行数・末尾
ika data sql "select ..." [--json]  # 読み取り専用で SELECT（テーブル名はデータセット名）。集計・結合もここで
ika data export <name>              # CSV で標準出力へ
ika data delete <name> --where "..."   # 行を消す（訂正は消して追記し直す）
ika data rm <name>                  # データセットごと消す
ika serve [--port 8787] [--open]   # KB をブラウザで読むローカルサーバー（読み取り専用。127.0.0.1 だけで待ち受け、Host は localhost・127.0.0.1 のみ）。毎回ファイルを読み直す。データセットも `/data/<name>` で読める（新しい順に最大200行）。タグ一覧は `/tags`、タグ別は `/tags/<タグ>`。検索画面は全文検索（`ika kb fts`）で、「文字列一致」にすると `ika kb search` と同じ探し方になる。検索語は結果の抜粋と、そこから開いたページ（`?hl=`）で黄色くハイライトする
ika check                       # KB・登録簿・データセット定義・図の記法の検証。14 日以内のノートでどこからもリンクされていないものは「注意」に出す（失敗にはしない）。リポジトリ内で実行するとプラグインも検証する
ika sync                        # docs/kb-format.md・templates/ をプラグインのスキルにコピー（リポジトリ内で実行）
ika install [--bump patch|minor|major] [--dry-run]   # （リポジトリ内で実行）このリポジトリを手元のプラグイン置き場として Claude・Codex の設定に登録し、ika プラグインを入れる（入っていれば更新する。公開はしない）
ika export claude [--with-kb] [--domain <領域>]... [--no-projects] [--with-sources]   # dist/claude/ika.zip（Upload plugin 用）。--with-kb で KB のスナップショット（knowledge-*.md・CSV・instructions.md）も出す。クラウドに上がるので範囲は --domain・--no-projects で絞る
ika export chatgpt [--domain <領域>]... [--no-projects]   # dist/chatgpt/（ChatGPT web のプロジェクト用。IKA_HOME の KB を束ねる。リポジトリ内で実行）
ika export html <kb/...md> | --project <name> | --all [--open]   # KB のページを、図（docs/diagrams.md）と表の入った単独の HTML にする。出力は IKA_HOME/.cache/html/（KB と同じ階層。--project は project-<name>.html に README・資料・要件書・決定・知見を束ね、--all は index.html が入口）。JS・外部参照なし、画像は埋め込み。図の記法に誤りがあれば書き出さない。どこからでも動く
```
