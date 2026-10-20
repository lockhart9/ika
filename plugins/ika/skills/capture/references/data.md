# データセット: 行が増え続ける観測を溜める

価格の履歴・計測値・ログのように、**同じ形の行が増え続け、集計して使う**ものは、ノートに書かずデータセット（SQLite の表）に溜める。読み書きは `ika data` だけで行い、`sqlite3` で直接触らない。

## ノートかデータセットか

- 「この週はこう動いた」という解釈・出来事 → note / insight
- 「いつ・どこで・何が・いくら」という観測の行 → データセット。そこから言えることは note / insight に書き、本文でデータセット名を引用する
- 数行の表で済むもの、一度きりの比較 → note の表でよい
- 迷ったら「あとで平均・推移・件数を出したいか」。出したいならデータセット

## 手順

1. 既存を探す: `ika data ls`、`ika data show <name>`。同じ種類のデータがあれば**新しく作らず追記する**
2. 無ければ作る。列は後から足せないので、最初に見通して決める
   ```bash
   ika data new prices --summary "対象ごとの価格の記録" \
     --col date:date --col 'source:enum(source_a|source_b)' --col item:string \
     --col price:int --col 'url:string?' \
     --key date,source,item --domain business --project <name>
   ```
   - 名前は英小文字で始まり、英小文字・数字・アンダースコアだけ
   - 型: `string` `int` `float` `date`（`YYYY-MM-DD`）`bool` `enum(a|b)`。既定は必須、省略可は型の後ろに `?`
   - `--key` は重複判定の列（必須の列だけ）。取り直しても二重に入らないよう、日付・出所・対象をキーにする
   - `--domain`・`--project` は登録済みの名前だけ（`ika domain list` / `ika project list`）
3. 追記する。標準入力にヘッダ付き CSV を渡す。列の順番は自由で、省略可の列は省ける
   ```bash
   ika data append prices --on-conflict skip <<'EOF'
   date,source,item,price
   2026-10-18,source_a,item-1,1200
   EOF
   ```
   - 全行を検証してから入れるので、不正な行があれば何も入らない。エラーの行番号を見て直す
   - `--on-conflict`: 既定の `error` は重複で全体を取り消す。取り直しなら `skip`、値の訂正なら `replace`
   - 日付は絶対日付。値が分からない行は作らず、省略可の列を空にする
4. 間違えて入れた行は `ika data delete <name> --where "..."` で消して入れ直す。表ごとなら `ika data rm <name>`
5. 出典（URL、取得日、取得方法）は、列（`url:string?` など）に持つか、解釈を書くノートの `source:` に書く

## 読む

```bash
ika data sql "select source, count(*) n, round(avg(price)) avg from prices group by source"
ika data sql "select date, price from prices where item = 'item-1' order by date" --json
```

- テーブル名はデータセット名。SELECT だけ実行できる（結合・集計も可）
- ブラウザで見るなら `ika serve`（`/data/<name>`）。CSV が要るなら `ika data export <name>`
