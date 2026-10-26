---
name: project-add
description: 新しいプロダクト（開発中のサービス、または事業・副業の検討）を ika のナレッジベースに登録する。「新しいサービスを始めた」「〜をKBに追加して」「プロダクト一覧に足して」のような依頼や、記録しようとした内容のプロダクトが未登録だったときに使う。
compatibility: KB（環境変数 IKA_HOME のディレクトリ）の読み書きには ika CLI が必要。Claude Code・Codex・ChatGPT デスクトップアプリ向け
---

# project-add: プロダクトを登録する

KB は ika のリポジトリではなく環境変数 `IKA_HOME` のディレクトリにある。未設定だと `ika` は動かないので、設定を頼む。読み書きは `ika` CLI で行い、ファイルを直接 Read/Write/rg しない。プロダクト一覧の正本は `data/projects.csv`。構成と書式は [references/kb-format.md](references/kb-format.md)、概要のひな形は [assets/project.md](assets/project.md)。
製品リポジトリには ika・KB への言及を書かない（ika 自身のリポジトリを除く）。コード・コメント・テスト名・ドキュメント・設定・コミットメッセージ・PR に、ika・KB・`kb/` のパス・要件書を書かず、出典は一次情報の URL だけにする。詳しくは [cycle の資料](../cycle/references/product-repo.md)。
KB にアクセスできない環境（claude.ai・ChatGPT web など）では、保存先のパスとフロントマター付きの本文を出力し、ユーザーに保存してもらう。

## 手順

1. 名前を決める。英小文字・数字・ハイフンのみ（リポジトリ名に合わせるのが基本）。`data/projects.csv` に重複がないか確かめる
2. 登録する
   ```bash
   ika project add <name> --path <リポジトリのパス> --url https://... --summary "一行説明"
   ```
   コードが無い事業・副業の検討は `--path` を付けない（リポジトリ欄は「なし」になる）。
   CLI が無ければ、`data/projects.csv` に行を足し、[assets/project.md](assets/project.md) から `kb/projects/<name>/README.md` を作る（`{{...}}` を埋める）
3. リポジトリがあれば `project-sync` スキルの手順で README を埋める。無ければ（企画段階など）ユーザーに目的・ターゲットを聞いて書く
4. `ika check` を通す
