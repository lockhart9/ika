# セキュリティポリシー / Security Policy

## 脆弱性の報告 / Reporting a vulnerability

脆弱性は公開の Issue や PR に書かず、GitHub の
[Private vulnerability reporting](https://github.com/lockhart9/ika/security/advisories/new)
から非公開で報告してください。

Please do not open a public issue or pull request for a vulnerability. Report it privately through
[GitHub's private vulnerability reporting](https://github.com/lockhart9/ika/security/advisories/new).

報告には次を含めてください。 / Please include:

- 影響を受けるコマンドやファイル / the affected command or file
- 再現手順 / steps to reproduce
- 想定される影響 / the expected impact

個人で運用しているため、対応時期は約束できません。確認でき次第、報告者に連絡します。
This is a personal project, so response times are best effort.

## 対象 / Scope

最新の `main` だけが対象です。次のような問題を特に重視します。

Only the latest `main` is supported. The following are of particular interest:

- `ika kb` 系コマンドが `IKA_HOME/kb/` の外を読み書きできる（パストラバーサル、シンボリックリンク）
- `ika kb grep` の許可リストをすり抜けて、外部コマンドの実行や `kb/` 外の読み取りができる
- `ika serve` が 127.0.0.1 以外から、または不正な Host ヘッダーで読める（DNS rebinding を含む）
- `ika export` が意図しないデータ（`--domain` や `--no-projects` で除外した範囲）を書き出す
- `ika data` の SQL 実行が読み取り専用の制約を超える
