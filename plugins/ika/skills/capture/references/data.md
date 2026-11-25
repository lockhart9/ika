# データセット: 行が増え続ける観測を溜める

価格の履歴・計測値・ログのように、**同じ形の行が増え続け、集計して使う**ものは、ノートに書かずデータセット（CSV の表）に溜める。読み書きは `ika data` だけで行い、`data/datasets/` の CSV を直接読み書きしない。

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
4. 間違えて入れた行は `ika data delete <name> -w "列=値"` で消して入れ直す（条件は下の `query` と同じ）。表ごとなら `ika data rm <name>`
5. 出典（URL、取得日、取得方法）は、列（`url:string?` など）に持つか、解釈を書くノートの `source:` に書く

## 読む

全行を読まない。要約を見てから、要る行・要る数字だけを出す。

```bash
ika data show prices                 # 定義・行数・列ごとの要約（範囲・平均・値ごとの件数）・末尾 5 行
ika data query prices --group-by source --agg count,avg:price
ika data query prices -w item=item-1 -w 'date>=2026-10-01' --cols date,price --sort date --json
```

- `-w` は `列 演算子 値`（`=` `!=` `<` `<=` `>` `>=` `~` 含む `!~`）。繰り返すと AND。値は列の型で比べる。`列=` は空欄の行
- `--agg` は `count`・`count:列`・`sum:列`・`avg:列`・`min:列`・`max:列`。出力の列名は `avg_price` のようになり、`--sort -avg_price` で並べ替えられる
- 既定で 50 行まで（超えた分は件数だけ出る）。全部要るときだけ `--limit 0`
- 結合・任意の式・OR はない。要るなら `query` を 2 回に分ける
- ブラウザで見るなら `ika serve`（`/data/<name>`）。CSV 全体が要るなら（移行・共有）`ika data export <name>`
