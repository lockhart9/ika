---
name: decide
description: プロダクトやビジネスの意思決定を、過去のナレッジを踏まえて整理し、決定ログとして ika に記録する。「どっちにすべき？」「〜を決めたい」「判断材料を整理して」「決めたので記録して」のような依頼で使う。
compatibility: KB（環境変数 IKA_HOME のディレクトリ）の読み書きには ika CLI が必要。Claude Code・Codex・ChatGPT デスクトップアプリ向け
---

# decide: 意思決定を支援し記録する

KB は ika のリポジトリではなく環境変数 `IKA_HOME` のディレクトリにある。未設定だと `ika` は動かないので、設定を頼む。読み書きは `ika` CLI で行い、ファイルを直接 Read/Write/rg しない。構成と書式は [references/kb-format.md](references/kb-format.md)、ひな形は [assets/decision.md](assets/decision.md)。
製品リポジトリには ika・KB への言及を書かない（ika 自身のリポジトリを除く）。コード・コメント・テスト名・ドキュメント・設定・コミットメッセージ・PR に、ika・KB・`kb/` のパス・要件書を書かず、出典は一次情報の URL だけにする。詳しくは [cycle の資料](../cycle/references/product-repo.md)。
KB にアクセスできない環境（claude.ai・ChatGPT web など）では、保存先のパスとフロントマター付きの本文を出力し、ユーザーに保存してもらう。

## 手順

1. 過去の経緯を集める（`search` スキルの手順）。特に同じプロダクトの `kb/*/decisions/` と関連する `kb/*/insights/`。技術の判断でもビジネス側の制約（費用・期限・顧客）を、ビジネスの判断でも技術側の制約を確認する
2. 論点を整理して示す
   - 決めること、期限、制約
   - 選択肢（最低2つ。「何もしない」も候補に入れる）と、それぞれの良い点・悪い点・コスト
   - 判断に効く KB の根拠（パス付き）と、足りない情報
   - 推奨案とその理由。ただし決めるのはユーザー
3. ユーザーが決めたら記録する
   ```bash
   ika new decision "タイトル" --domain <business|tech> --project <name> --slug <英字、内容が分かる名前> --stdin <<'EOF'
   ...本文（背景・選択肢・決定・理由・見直す条件）...
   EOF
   ```
   作った直後に「似た既存のナレッジ」が標準エラーに出たら、同じ論点の決定がすでにないか確かめる（あれば新しく作らず、覆すなら手順 4）。
   `status` などフロントマターの直しは `ika kb edit <path> --old "status: proposed" --new "status: accepted"`
   - 領域は「何についての決定か」で選ぶ（価格・ターゲット・撤退 → `business`、技術選定・設計・移行 → `tech`）
   - `status`: ユーザーが決めた → `accepted`、まだ案 → `proposed`、見送り → `rejected`
   - 「見直す条件」に、何が起きたら再検討するかを具体的に書く
4. 以前の決定を覆したなら、古い方の `status` を `superseded` にし（`ika kb edit`）、本文末尾に新しい決定へのリンクを足す（`ika kb append`）
5. `kb/projects/<name>/README.md` の「関連」の該当する領域（ビジネス／技術）にリンクを足し（`ika kb edit`）、`ika check` を通す
