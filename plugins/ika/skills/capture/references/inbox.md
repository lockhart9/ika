# 受信箱を整理する

`ika capture "メモ"` で領域・プロダクト・タグを決めずに溜めた生のメモ（URL を含む）を、KB のノートに振り分ける手順。受信箱は `IKA_HOME/inbox/YYYY-MM-DD.md` にあり、`kb/` の外なので `ika kb` では読めない。読むのは `ika inbox ls`、消すのは `ika inbox rm <日付>`。

## いつやるか

- `ika kb triage` の「今週やること」に【受信箱】が出たとき
- ユーザーが「受信箱を整理して」「溜めたメモを片付けて」と頼んだとき

## 手順

1. `ika inbox ls` で全部読む（日付と時刻つきで全文が出る）。件数が多いときは `--date YYYY-MM-DD` で1日ずつ
2. 項目ごとに振り分ける。意図が読み取れないメモは、勝手に捨てずにユーザーに聞く
   - ノートにする: 本文の手順（[SKILL.md](../SKILL.md) の 1〜6）で領域・プロダクト・タグを決めて `ika new note`。同じ話題のノートがすでにあれば `ika kb search` で探し、`ika kb append` で足す
   - URL だけのメモ: ページの本文を取って読み、要点をノートにする。全文を残したい資料なら、取った本文を `ika import - --source <URL> --title "..." --project <name>`（または `--domain`）で `sources/` に置き、要点をノートに書く（[import スキル](../../import/SKILL.md)）。取れないページ（ログイン必須など）は、URL と分かる範囲の要約を「未確認」と明記してノートにする
   - 捨てる: 重複・もう要らないもの。ユーザーに「捨てる」と伝えてから消す
3. その日の項目をすべて振り分けたら `ika inbox rm <日付>`。一部だけ済んだ日は、残りを `ika capture` で入れ直してから `ika inbox rm`（日ごとのファイル単位でしか消せない）。消しても履歴は IKA_HOME の git に残る
4. 作ったノートの `ika kb orphans -p <name>` に「要整理」が出ていれば、[distill.md](distill.md) の手順で insight への蒸留を提案する
5. `ika check` を通す
