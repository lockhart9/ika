---
name: project-sync
description: 開発中のサービスのリポジトリ（data/projects.csv の path）を読み、ika のプロダクト概要（kb/projects/<name>/README.md）を最新の状態に更新する。「〜の現状をKBに反映して」「プロダクト概要を更新」「全プロダクトの棚卸し」のような依頼で使う。
compatibility: KB（環境変数 IKA_HOME のディレクトリ）の読み書きには ika CLI が必要。Claude Code・Codex・ChatGPT デスクトップアプリ向け
---

# project-sync: プロダクト概要をリポジトリと同期する

`path` が空のプロダクト（事業・副業の検討など）はリポジトリが無いので対象外。README は `capture`・`decide` の結果やユーザーへの聞き取りで更新する。

KB は ika のリポジトリではなく環境変数 `IKA_HOME` のディレクトリにある。未設定だと `ika` は動かないので、設定を頼む。読み書きは `ika` CLI で行い、ファイルを直接 Read/Write/rg しない。対象リポジトリのパスは `data/projects.csv` の `path`。構成と書式は [references/kb-format.md](references/kb-format.md)。

## 手順

1. 対象を決める（指定がなければ `status` が `active` のもの全部）
2. リポジトリを読む（書き換えない）
   - `README.md`、`CLAUDE.md` / `.claude/CLAUDE.md`、`AGENTS.md`、`docs/`
   - `git -C <path> log --since=<前回の date> --oneline` で前回更新以降の動き
   - `package.json` / `Cargo.toml` やデプロイ設定などで技術構成と公開ドメイン
3. `kb/projects/<name>/README.md` を更新する。対象リポジトリで作業中でも `ika kb cat kb/projects/<name>/README.md` で読み、`ika kb edit`（部分置換）か `ika kb write --overwrite`（全文）で書く。KB のファイルを直接 Write/Edit しない
   - 「現状」に直近の動きを日付付きで1〜3行
   - 「技術」の変化（メジャーバージョン、構成変更）
   - 「課題・次の打ち手」はリポジトリの TODO・docs から拾い、推測なら「推測」と書く。`cycle` スキルが次のテーマに使うので、1行1件で何をなぜやるかが分かるように書く
   - `ika kb search --type spec --status approved -p <name> -l` で承認済みの要件書を確かめ、実装済みなのに `approved` のままなら `done` にして「実装の記録」を足す
   - フロントマターの `date` を今日にする
4. URL や一行説明が変わっていたら `ika project set <name> --url ... --summary ...` で `data/projects.csv` を直す（CSV を直接編集しない）
5. 複数プロダクトに共通する気づきがあれば `capture` スキルで insight を提案する（同じ技術課題・運用の手間 → `tech`、同じ集客・収益の課題 → `business`）
6. `ika check` を通し、更新内容を要約して報告する
