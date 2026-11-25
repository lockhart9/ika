# data — 表形式データ

`IKA_HOME/data/` に置く（ika のリポジトリではない）。登録簿も、溜めていくデータ（データセット）も CSV。

- CSV は UTF-8・ヘッダ行あり・カンマ区切り。日付は `YYYY-MM-DD`
- JSON は1ファイル1データセット。配列かオブジェクトのどちらかに揃える
- 新しいデータセットを足したら、下の表に1行書く

| ファイル | 内容 | 列 |
|---|---|---|
| `projects.csv` | プロダクト登録簿（正本） | `name`（英小文字・ハイフン）, `path`（ローカルリポジトリ）, `url`（公開URL、無ければ空）, `status`（`active` / `paused` / `archived`）, `summary` |
| `domains.csv` | 領域登録簿（正本）。`kb/<name>/` と対応 | `name`（英小文字・ハイフン、`projects` は不可）, `summary` |
| `externals.csv` | 外部ナレッジの目録。KB の外の Drive・SaaS・手元のフォルダ・Web の「どこに何があり、いつ見るか」。中身と資格情報は持たない。無ければ登録なし（`ika external add` が作る） | `name`（英小文字・ハイフン）, `kind`（`connector` / `folder` / `web`）, `location`（URL・絶対パス、`~` 可）, `connector`（`connector` のとき必須。クライアントでの表示名）, `projects`・`domains`（`;` 区切り。空はすべて）, `about`（何が入っていて、いつ見るか） |

## データセット（`datasets.csv`・`datasets/<name>.csv`）

価格の履歴・計測値・ログのように、行が増え続けて集計して使うデータはデータセットに溜める。

- 定義は登録簿 `data/datasets.csv`（1 データセット 1 行）。列は `name`, `summary`, `domain`, `projects`（`;` 区切り）, `key`（`;` 区切り）, `columns`（`date:date;kind:enum(sold|listed);url:string?` のように `;` 区切り）, `created`
- 行は `data/datasets/<name>.csv`。ヘッダは列名だけの普通の CSV で、並びは追記した順
- 値の書き方: `int`・`float` は数のまま、`date` は `YYYY-MM-DD`、`bool` は `true` / `false`、`enum`・`string` はそのまま、省略可の列の空は空欄
- 名前（データセット・列）は英小文字で始まり、英小文字・数字・アンダースコアだけ（ファイル名と条件・集計の中でそのまま使う）
- 型は `string` / `int` / `float` / `date`（`YYYY-MM-DD`）/ `bool` / `enum(a|b)`。既定は必須で、省略可は `?`（`url:string?`）。enum の値に `;` は使えない
- `--key` は重複判定の列（必須の列だけ）。観測を取り直しても二重に入らないよう、日付・出所・対象をキーにする
- `--domain`・`--project` で、どの領域・プロダクトのデータかを残す（登録済みの名前だけ）

読み書きは `ika data`（と `ika serve`）を通す。ファイルはテキストなので git の差分で読めるが、書き込みは検証を通すため手で直さない（手で直したものは `ika check` が型・key の重複・ヘッダの食い違いを指摘する）。

- 追記は標準入力のヘッダ付き CSV。全行を検証してから書くので、不正な行があれば何も入らない。書き込みは一時ファイルに書いてから置き換える
- 訂正は `ika data delete -w ...` で消して追記し直す。履歴は git（`IKA_HOME` の自動コミット）に残る
- 読むときは全行を出さない。`ika data show` で列ごとの要約（空欄の数・最小/最大/平均・値ごとの件数・種類の数）を見て、`ika data query` で絞り込み（`-w`）・列の選択・グループ集計（`--group-by`・`--agg`）・並べ替え・上限（既定 50 行）を付けて要る分だけ出す。データセットをまたぐ結合や任意の式はない
- CSV 全体は `ika data export` で書き出せる（移行・共有用）。`ika export --with-kb` のスナップショットには含めない

データセットの一覧と定義は `ika data ls` / `ika data show <name>`。このファイルに表を書いて管理しない。

### 以前の形式から移す

以前の版は `data/datasets.sqlite3`（SQLite）に持っていた。残っていると `ika check` が知らせる。OS の `sqlite3` コマンドで、データセットごとに定義を見て作り直し、行を入れ直す。

```bash
cd "$IKA_HOME"
sqlite3 -csv -header data/datasets.sqlite3 "select name, summary, domain, projects, key, columns from _datasets"
# 定義（columns は JSON）を見て、同じ名前・列・key で作る
ika data new <name> --summary "..." --col ... --key ... [--domain ...] [--project ...]
sqlite3 -csv -header data/datasets.sqlite3 "select * from <name> order by rowid" | ika data append <name>
ika data show <name>          # 行数を確かめる
git rm -q data/datasets.sqlite3 && git commit -qm "data: CSV に移した"
```
