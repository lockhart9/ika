//! `ika` バイナリを一時ディレクトリの KB（IKA_HOME）に対して実行する統合テスト。
//! 外部プロジェクトの作業中（ika の外のカレント）から使う想定で、cwd は KB の外にする。

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

struct Fixture {
    dir: PathBuf,
}

struct Run {
    ok: bool,
    out: String,
    err: String,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("ika-it-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let fixture = Fixture { dir };
        let root = fixture.root();
        for sub in [
            "data",
            "kb/tech/notes/2026",
            "kb/tech/insights",
            "kb/projects/foo",
        ] {
            fs::create_dir_all(root.join(sub)).unwrap();
        }
        fs::create_dir_all(fixture.dir.join("work")).unwrap();
        fs::write(
            root.join("data/projects.csv"),
            "name,path,url,status,summary\nfoo,/path/to/foo,,active,Foo\n",
        )
        .unwrap();
        fs::write(root.join("data/domains.csv"), "name,summary\ntech,t\n").unwrap();
        fixture.write(
            "kb/projects/foo/README.md",
            "---\ntitle: foo\ntype: project\ndate: 2026-10-14\nprojects: [foo]\ntags: []\n---\n\n# foo\n",
        );
        fixture
    }

    /// KB が無い状態（`ika init` 前）。IKA_HOME は存在しないディレクトリを指す
    fn bare(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("ika-it-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("work")).unwrap();
        Fixture { dir }
    }

    /// KB のデータディレクトリ（IKA_HOME）
    fn root(&self) -> PathBuf {
        self.dir.join("ika-home")
    }

    fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(self.root())
            .args(args)
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap()
    }

    fn write(&self, rel: &str, text: &str) {
        let path = self.root().join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.root().join(rel)).unwrap()
    }

    fn exists(&self, rel: &str) -> bool {
        self.root().join(rel).exists()
    }

    fn ika(&self, args: &[&str]) -> Run {
        self.ika_with(args, None)
    }

    fn ika_with(&self, args: &[&str], stdin: Option<&str>) -> Run {
        self.run(args, stdin, Some(self.root()))
    }

    /// `home` が `None` なら IKA_HOME を設定しない
    fn run(&self, args: &[&str], stdin: Option<&str>, home: Option<PathBuf>) -> Run {
        self.run_in(&self.dir.join("work"), args, stdin, home)
    }

    fn run_in(
        &self,
        cwd: &std::path::Path,
        args: &[&str],
        stdin: Option<&str>,
        home: Option<PathBuf>,
    ) -> Run {
        let mut command = Command::new(env!("CARGO_BIN_EXE_ika"));
        command.args(args).env_remove("IKA_HOME");
        if let Some(home) = home {
            command.env("IKA_HOME", home);
        }
        let mut child = command
            // git のコミットに必要な名前（HOME が空なので設定が無い）
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .env("HOME", &self.dir)
            .current_dir(cwd)
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        if let Some(text) = stdin {
            child
                .stdin
                .take()
                .unwrap()
                .write_all(text.as_bytes())
                .unwrap();
        }
        let Output {
            status,
            stdout,
            stderr,
        } = child.wait_with_output().unwrap();
        Run {
            ok: status.success(),
            out: String::from_utf8(stdout).unwrap(),
            err: String::from_utf8(stderr).unwrap(),
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

const NOTE: &str =
    "---\ntitle: n\ntype: note\ndate: 2026-10-14\nprojects: [foo]\ntags: [auth]\n---\n\n本文\n";

#[test]
fn new_takes_body_from_stdin_and_keeps_frontmatter() {
    let f = Fixture::new("new");
    let run = f.ika_with(
        &[
            "new",
            "note",
            "Review notes",
            "--domain",
            "tech",
            "--project",
            "foo",
            "--date",
            "2026-10-14",
            "--stdin",
        ],
        Some("## 要点\n\n- 標準入力の本文\n"),
    );
    assert!(run.ok, "{}", run.err);
    let text = f.read("kb/tech/notes/2026/2026-10-14-review-notes.md");
    assert!(
        text.starts_with("---\ntitle: Review notes\ntype: note\ndate: 2026-10-14\nprojects: [foo]")
    );
    assert!(text.contains("- 標準入力の本文"));
    assert!(
        !text.contains("次にやること"),
        "テンプレートの本文は入らない"
    );
}

#[test]
fn write_refuses_to_overwrite_unless_asked() {
    let f = Fixture::new("write");
    let path = "kb/tech/notes/2026/2026-10-14-a.md";
    assert!(f.ika_with(&["kb", "write", path], Some(NOTE)).ok);
    assert_eq!(f.read(path), NOTE);

    let again = f.ika_with(&["kb", "write", path], Some("changed\n"));
    assert!(!again.ok);
    assert_eq!(f.read(path), NOTE);

    assert!(
        f.ika_with(&["kb", "write", path, "--overwrite"], Some(NOTE))
            .ok
    );
}

#[test]
fn write_reports_problems_but_succeeds() {
    let f = Fixture::new("report");
    let run = f.ika_with(
        &["kb", "write", "kb/tech/notes/2026/2026-10-14-bad.md"],
        Some("フロントマターなし\n"),
    );
    assert!(run.ok);
    assert!(run.out.contains("要修正"), "{}", run.out);
    assert!(run.out.contains("フロントマターがありません"));
}

#[test]
fn edit_needs_a_unique_match_unless_all() {
    let f = Fixture::new("edit");
    let path = "kb/tech/notes/2026/2026-10-14-a.md";
    f.write(path, &NOTE.replace("本文", "同じ 同じ"));

    let ambiguous = f.ika(&["kb", "edit", path, "--old", "同じ", "--new", "違う"]);
    assert!(!ambiguous.ok);
    assert!(ambiguous.err.contains("2 か所"), "{}", ambiguous.err);
    assert!(f.read(path).contains("同じ 同じ"));

    assert!(
        !f.ika(&["kb", "edit", path, "--old", "無い", "--new", "x"])
            .ok
    );
    assert!(
        f.ika(&[
            "kb", "edit", path, "--old", "同じ", "--new", "違う", "--all"
        ])
        .ok
    );
    assert!(f.read(path).contains("違う 違う"));
}

#[test]
fn append_adds_to_the_end() {
    let f = Fixture::new("append");
    let path = "kb/tech/notes/2026/2026-10-14-a.md";
    f.write(path, NOTE);
    assert!(f.ika_with(&["kb", "append", path], Some("追記\n")).ok);
    assert_eq!(f.read(path), format!("{NOTE}追記\n"));
}

#[test]
fn closed_stdout_pipe_ends_quietly() {
    let f = Fixture::new("closed-pipe");
    let path = "kb/tech/notes/2026/2026-10-14-big.md";
    // パイプのバッファより大きくして、読み手が閉じたあとにも書き続けるようにする
    f.write(path, &format!("{NOTE}{}", "あいうえお\n".repeat(50_000)));
    let mut child = Command::new(env!("CARGO_BIN_EXE_ika"))
        .args(["kb", "cat", path])
        .env("IKA_HOME", f.root())
        .env("HOME", &f.dir)
        .current_dir(f.dir.join("work"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // `ika ... | head` と同じ。何も読まずに閉じる
    drop(child.stdout.take());
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(141));
    assert_eq!(String::from_utf8_lossy(&out.stderr), "");
}

#[test]
fn cat_ls_and_search_work_from_outside() {
    let f = Fixture::new("read");
    f.write("kb/tech/notes/2026/2026-10-14-a.md", NOTE);
    f.write(
        "kb/tech/notes/2026/2026-10-15-b.md",
        &NOTE
            .replace("projects: [foo]", "projects: []")
            .replace("auth", "infra"),
    );

    assert!(
        f.ika(&["kb", "cat", "kb/tech/notes/2026/2026-10-14-a.md"])
            .out
            .contains("本文")
    );
    let ls = f.ika(&["kb", "ls", "kb/tech"]);
    assert_eq!(ls.out.lines().count(), 2, "{}", ls.out);

    let by_project = f.ika(&["kb", "search", "-p", "foo", "-l"]);
    assert_eq!(
        by_project.out.trim(),
        "kb/projects/foo/README.md\nkb/tech/notes/2026/2026-10-14-a.md"
    );
    let by_tag = f.ika(&["kb", "search", "本文", "-t", "infra", "-l"]);
    assert_eq!(by_tag.out.trim(), "kb/tech/notes/2026/2026-10-15-b.md");
    assert!(
        f.ika(&["kb", "search", "存在しない語", "-l"])
            .out
            .is_empty()
    );
}

#[test]
fn kb_fts_matches_morphemes_and_substrings_and_keeps_the_index_out_of_git() {
    let f = Fixture::bare("fts");
    assert!(f.ika(&["init"]).ok);
    assert!(f.ika(&["domain", "add", "tech", "--summary", "t"]).ok);
    let note = |kind: &str, body: &str| {
        format!(
            "---\ntitle: n\ntype: {kind}\ndate: 2026-10-14\nprojects: []\ntags: []\n---\n\n{body}\n"
        )
    };
    let a = "kb/tech/decisions/2026-10-14-withdraw.md";
    let b = "kb/tech/notes/2026/2026-10-14-dynamo.md";
    let c = "kb/tech/notes/2026/2026-10-15-list.md";
    for (path, text) in [
        (a, note("decision", "未処理の申請は本人が取り下げられる。")),
        (
            b,
            note("note", "DynamoDB のオンデマンドモードに切り替える。"),
        ),
        (c, note("note", "申請の一覧を表示する。")),
    ] {
        let run = f.ika_with(&["kb", "write", path], Some(&text));
        assert!(run.ok, "{}", run.err);
    }
    let fts = |args: &[&str]| {
        let mut all = vec!["kb", "fts", "-l"];
        all.extend_from_slice(args);
        let run = f.ika(&all);
        assert!(run.ok, "{}", run.err);
        let mut lines: Vec<String> = run.out.lines().map(String::from).collect();
        lines.sort();
        lines
    };

    // 文で引いても、助詞・活用を除いた形態素で当たる（「申請」だけの c には当たらない）
    assert_eq!(fts(&["申請を取り下げたい"]), [a]);
    // 形態素の途中でも、大文字小文字を問わず部分一致で当たる
    assert_eq!(fts(&["オンデマンド"]), [b]);
    assert_eq!(fts(&["dynamo"]), [b]);
    assert_eq!(fts(&["申請", "--type", "note"]), [c]);
    assert_eq!(fts(&["申請", "dynamodb", "--any"]), [a, b, c]);
    assert_eq!(fts(&["申請", "dynamodb", "--any", "-n", "1"]).len(), 1);
    // 検索語なしは一覧なので、件数を絞らない（新しい日付順。同じ日付ならパス順）
    let listing = f.ika(&["kb", "fts", "--type", "note", "-l"]);
    assert_eq!(listing.out.lines().collect::<Vec<_>>(), [b, c]);
    assert!(fts(&["の"]).is_empty(), "助詞1文字では何も引かない");

    // 索引は IKA_HOME/.cache/ に置き、git には入れない
    assert!(f.exists(".cache/fts/manifest.json"));
    assert_eq!(f.git(&["status", "--porcelain"]).trim(), "");

    // 書き換え・削除は次の検索で索引に反映される
    assert!(
        f.ika(&[
            "kb",
            "edit",
            c,
            "--old",
            "一覧を表示",
            "--new",
            "取り下げを表示"
        ])
        .ok
    );
    assert!(f.ika(&["kb", "rm", a]).ok);
    assert_eq!(fts(&["申請を取り下げたい"]), [c]);

    // 索引の更新と作り直しは単独でもできる
    let update = f.ika(&["kb", "index"]);
    assert!(update.ok && update.out.contains("追加 0"), "{}", update.out);
    let rebuild = f.ika(&["kb", "index", "--rebuild"]);
    assert!(
        rebuild.ok && rebuild.out.contains("作り直しました") && rebuild.out.contains("索引 2 件"),
        "{}",
        rebuild.out
    );
    assert_eq!(fts(&["申請を取り下げたい"]), [c]);
}

#[test]
fn kb_stale_finds_outdated_readmes_insights_evidence_and_sources() {
    let f = Fixture::new("stale");
    // プロダクト foo のリポジトリ。README（2026-10-14）より後にコミットがある
    let repo = f.dir.join("repo");
    fs::create_dir_all(&repo).unwrap();
    let git = |args: &[&str]| {
        let ok = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.test"])
            .args(args)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?}");
    };
    git(&["init", "-q"]);
    git(&["commit", "-q", "--allow-empty", "-m", "after readme"]);
    f.write(
        "data/projects.csv",
        &format!(
            "name,path,url,status,summary\nfoo,{},,active,Foo\nbar,/nowhere/bar,,active,Bar\n",
            repo.display()
        ),
    );
    f.write(
        "kb/projects/bar/README.md",
        "---\ntitle: bar\ntype: project\ndate: 2026-10-14\nprojects: [bar]\ntags: []\n---\n\n# bar\n",
    );

    let note = |date: &str, tags: &str| {
        format!(
            "---\ntitle: n {date}\ntype: note\ndate: {date}\nprojects: [foo]\ntags: [{tags}]\n---\n\n本文\n"
        )
    };
    // 知見（2026-10-01）より新しいノートを取り込んでいない
    f.write(
        "kb/tech/notes/2026/2026-09-01-old.md",
        &note("2026-09-01", "auth"),
    );
    f.write(
        "kb/tech/notes/2026/2026-10-05-new.md",
        &note("2026-10-05", "auth"),
    );
    f.write(
        "kb/tech/notes/2026/2026-10-06-other.md",
        &note("2026-10-06", "infra"),
    );
    f.write(
        "kb/tech/insights/auth.md",
        "---\ntitle: 認証の知見\ntype: insight\ndate: 2026-10-01\nprojects: [foo]\ntags: [auth]\n---\n\n[根拠](../notes/2026/2026-09-01-old.md)\n",
    );
    // 古い価格のノートを、採用済みの決定が根拠にしている（却下した決定は数えない）
    f.write(
        "kb/tech/notes/2020/2020-01-01-price.md",
        &note("2020-01-01", "pricing"),
    );
    f.write(
        "kb/tech/notes/2020/2020-01-02-price.md",
        &note("2020-01-02", "pricing"),
    );
    let decision = |status: &str, target: &str| {
        format!(
            "---\ntitle: d\ntype: decision\ndate: 2020-02-01\nprojects: [foo]\ntags: []\nstatus: {status}\n---\n\n[根拠](../notes/2020/{target})\n"
        )
    };
    f.write(
        "kb/tech/decisions/2020-02-01-a.md",
        &decision("accepted", "2020-01-01-price.md"),
    );
    f.write(
        "kb/tech/decisions/2020-02-01-b.md",
        &decision("rejected", "2020-01-02-price.md"),
    );
    // 取り込んだ資料: 原本が変わったもの・消えたもの・変わっていないもの
    let originals = f.dir.join("originals");
    fs::create_dir_all(&originals).unwrap();
    for name in ["changed.md", "same.md", "gone.md"] {
        fs::write(originals.join(name), format!("# {name}\n")).unwrap();
        let run = f.ika(&[
            "import",
            originals.join(name).to_str().unwrap(),
            "--project",
            "foo",
        ]);
        assert!(run.ok, "{}", run.err);
    }
    fs::write(originals.join("changed.md"), "# changed v2\n").unwrap();
    fs::remove_file(originals.join("gone.md")).unwrap();

    let run = f.ika(&["kb", "stale"]);
    assert!(run.ok, "{}", run.err);
    let out = run.out;
    // 1. README（リポジトリが手元に無い bar は確かめない）
    assert!(
        out.contains("foo  README 2026-10-14 · その後のコミット 1 件"),
        "{out}"
    );
    assert!(!out.contains("bar  README"), "{out}");
    // 2. 知見
    assert!(
        out.contains("認証の知見") && out.contains("2026-10-05-new.md"),
        "{out}"
    );
    assert!(
        !out.contains("2026-10-06-other.md"),
        "タグが重ならないノートは数えない"
    );
    // 3. 古い価格
    assert!(
        out.contains("2020-01-01-price.md") && out.contains("2020-02-01-a.md"),
        "{out}"
    );
    assert!(
        !out.contains("2020-01-02-price.md"),
        "却下した決定の根拠は数えない"
    );
    // 4. 原本
    assert!(
        out.contains("原本が変わりました") && out.contains("changed.md"),
        "{out}"
    );
    assert!(
        out.contains("原本が見当たりません") && out.contains("gone.md"),
        "{out}"
    );
    assert!(!out.contains("same.md"), "{out}");

    // 日数とタグを変えられる。プロダクトで絞れる
    let narrow = f.ika(&["kb", "stale", "--volatile", "cost", "-p", "bar"]);
    assert_eq!(
        narrow.out.trim(),
        "古くなっていそうなものは見つかりませんでした"
    );
}

const LONG_NOTE: &str = "---\ntitle: n\ntype: note\ndate: 2026-10-14\nprojects: [foo]\ntags: [auth]\n---\n\n## 要点\n\n要点の本文\n\n## 詳細\n\n詳細の本文\n\n### 補足\n\n補足の本文\n\n## 詳細\n\n二つ目\n";

#[test]
fn cat_reads_only_a_part() {
    let f = Fixture::new("cat-part");
    let path = "kb/tech/notes/2026/2026-10-14-a.md";
    f.write(path, LONG_NOTE);

    let outline = f.ika(&["kb", "cat", path, "--outline"]);
    assert!(outline.ok, "{}", outline.err);
    assert!(
        outline.out.starts_with(&format!("{path}  全 ")),
        "{}",
        outline.out
    );
    assert!(outline.out.contains("L1-7"), "{}", outline.out);
    assert!(outline.out.contains("## 要点"), "{}", outline.out);
    assert!(outline.out.contains("### 補足"), "{}", outline.out);

    assert_eq!(
        f.ika(&["kb", "cat", path, "--section", "要点"]).out,
        "## 要点\n\n要点の本文\n\n"
    );
    assert_eq!(
        f.ika(&["kb", "cat", path, "--section", "### 補足"]).out,
        "### 補足\n\n補足の本文\n\n"
    );
    // 同名の見出しが複数あれば、候補を示して失敗する
    let ambiguous = f.ika(&["kb", "cat", path, "--section", "詳細"]);
    assert!(!ambiguous.ok);
    assert!(ambiguous.err.contains("2 か所"), "{}", ambiguous.err);
    let missing = f.ika(&["kb", "cat", path, "--section", "無い"]);
    assert!(!missing.ok);
    assert!(missing.err.contains("## 要点"), "{}", missing.err);

    assert_eq!(
        f.ika(&["kb", "cat", path, "--lines", "9:11"]).out,
        "## 要点\n\n要点の本文\n"
    );
    assert_eq!(f.ika(&["kb", "cat", path, "--lines", "9"]).out, "## 要点\n");
    assert!(!f.ika(&["kb", "cat", path, "--lines", "999"]).ok);
    assert!(!f.ika(&["kb", "cat", path, "--lines", "0:3"]).ok);

    assert_eq!(
        f.ika(&["kb", "cat", path, "--frontmatter"]).out,
        "---\ntitle: n\ntype: note\ndate: 2026-10-14\nprojects: [foo]\ntags: [auth]\n---\n"
    );

    // 部分指定は同時に1つだけ
    assert!(
        !f.ika(&["kb", "cat", path, "--outline", "--lines", "1:2"])
            .ok
    );
    // 部分指定も kb/ の外は読めない
    assert!(!f.ika(&["kb", "cat", "data/projects.csv", "--outline"]).ok);
}

#[test]
fn paths_outside_kb_are_rejected() {
    let f = Fixture::new("escape");
    let outside = f.dir.join("work/secret.md");
    fs::write(&outside, "secret").unwrap();
    for path in [
        "data/projects.csv",
        "kb/../data/projects.csv",
        outside.to_str().unwrap(),
    ] {
        for args in [vec!["kb", "cat", path], vec!["kb", "rm", path]] {
            assert!(!f.ika(&args).ok, "{args:?}");
        }
        assert!(
            !f.ika_with(&["kb", "write", path, "--overwrite"], Some("x\n"))
                .ok
        );
    }
    assert_eq!(fs::read_to_string(&outside).unwrap(), "secret");
    assert!(f.read("data/projects.csv").contains("foo"));
}

#[cfg(unix)]
#[test]
fn symlinks_in_kb_are_not_followed() {
    use std::os::unix::fs::symlink;

    let f = Fixture::new("symlink");
    let outside = f.dir.join("work/new.md");
    let link = f.root().join("kb/tech/dangling.md");
    symlink(&outside, &link).unwrap();

    let run = f.ika_with(
        &["kb", "write", "kb/tech/dangling.md", "--overwrite"],
        Some("x\n"),
    );
    assert!(!run.ok);
    assert!(!outside.exists(), "リンク先に書き出してはいけない");
}

#[test]
fn rm_deletes_files_but_not_registered_readmes() {
    let f = Fixture::new("rm");
    let path = "kb/tech/notes/2026/2026-10-14-a.md";
    f.write(path, NOTE);
    let run = f.ika(&["kb", "rm", path]);
    assert!(run.ok, "{}", run.err);
    assert!(!f.exists(path));
    assert!(!f.ika(&["kb", "rm", path]).ok, "2回目は存在しないので失敗");

    let readme = f.ika(&["kb", "rm", "kb/projects/foo/README.md"]);
    assert!(!readme.ok);
    assert!(f.exists("kb/projects/foo/README.md"));
    assert!(
        !f.ika(&["kb", "rm", "kb/tech"]).ok,
        "ディレクトリは消せない"
    );
}

#[test]
fn rm_lists_notes_that_link_to_the_removed_file() {
    let f = Fixture::new("rm-links");
    f.write("kb/tech/notes/2026/2026-10-14-a.md", NOTE);
    f.write(
        "kb/tech/insights/i.md",
        &NOTE.replace("本文", "根拠は[a](../notes/2026/2026-10-14-a.md)"),
    );
    f.write("kb/tech/insights/unrelated.md", NOTE);

    let run = f.ika(&["kb", "rm", "kb/tech/notes/2026/2026-10-14-a.md"]);
    assert!(run.ok);
    assert!(run.out.contains("- kb/tech/insights/i.md"), "{}", run.out);
    assert!(!run.out.contains("unrelated"));
}

#[test]
fn mv_moves_and_reports_broken_links() {
    let f = Fixture::new("mv");
    let from = "kb/tech/notes/2026/2026-10-14-a.md";
    let to = "kb/tech/notes/2026/2026-10-14-renamed.md";
    f.write(from, NOTE);
    f.write(
        "kb/tech/insights/i.md",
        &NOTE.replace("本文", "根拠は[a](../notes/2026/2026-10-14-a.md)"),
    );

    let run = f.ika(&["kb", "mv", from, to]);
    assert!(run.ok, "{}", run.err);
    assert!(!f.exists(from));
    assert_eq!(f.read(to), NOTE);
    assert!(run.out.contains("- kb/tech/insights/i.md"), "{}", run.out);
}

#[test]
fn mv_reports_links_that_break_in_the_moved_file() {
    let f = Fixture::new("mv-own");
    f.write("kb/tech/insights/i.md", NOTE);
    f.write(
        "kb/tech/notes/2026/2026-10-14-a.md",
        &NOTE.replace("本文", "[i](../../insights/i.md)"),
    );
    let run = f.ika(&[
        "kb",
        "mv",
        "kb/tech/notes/2026/2026-10-14-a.md",
        "kb/tech/insights/a.md",
    ]);
    assert!(run.ok);
    assert!(run.out.contains("リンク先がありません"), "{}", run.out);
}

#[test]
fn mv_refuses_existing_target_and_registered_readme() {
    let f = Fixture::new("mv-refuse");
    f.write("kb/tech/notes/2026/2026-10-14-a.md", NOTE);
    f.write("kb/tech/notes/2026/2026-10-14-b.md", NOTE);
    assert!(
        !f.ika(&[
            "kb",
            "mv",
            "kb/tech/notes/2026/2026-10-14-a.md",
            "kb/tech/notes/2026/2026-10-14-b.md"
        ])
        .ok
    );
    assert!(f.exists("kb/tech/notes/2026/2026-10-14-a.md"));
    assert!(
        !f.ika(&["kb", "mv", "kb/projects/foo/README.md", "kb/tech/foo.md"])
            .ok
    );
    assert!(f.exists("kb/projects/foo/README.md"));
}

#[test]
fn project_set_changes_only_given_fields() {
    let f = Fixture::new("project-set");
    let run = f.ika(&["project", "set", "foo", "--status", "paused"]);
    assert!(run.ok, "{}", run.err);
    assert_eq!(
        f.read("data/projects.csv"),
        "name,path,url,status,summary\nfoo,/path/to/foo,,paused,Foo\n"
    );
    assert!(!f.ika(&["project", "set", "foo"]).ok);
    assert!(!f.ika(&["project", "set", "nope", "--url", "x"]).ok);
    assert!(!f.ika(&["project", "set", "foo", "--status", "bad"]).ok);
    assert!(f.read("data/projects.csv").contains("paused"));
}

#[test]
fn no_temp_files_are_left_behind() {
    let f = Fixture::new("temp");
    let path = "kb/tech/notes/2026/2026-10-14-a.md";
    f.ika_with(&["kb", "write", path], Some(NOTE));
    f.ika(&["kb", "edit", path, "--old", "本文", "--new", "変更"]);
    f.ika_with(&["kb", "append", path], Some("x\n"));
    let names: Vec<_> = fs::read_dir(f.root().join("kb/tech/notes/2026"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["2026-10-14-a.md"]);
}

#[test]
fn new_needs_a_slug_for_japanese_only_titles() {
    let f = Fixture::new("slug");
    let args = [
        "new",
        "note",
        "料金の見直し",
        "--domain",
        "tech",
        "--date",
        "2026-10-14",
    ];

    let run = f.ika_with(&args, Some(""));
    assert!(!run.ok);
    assert!(run.err.contains("--slug"), "{}", run.err);
    assert_eq!(
        fs::read_dir(f.root().join("kb/tech/notes"))
            .unwrap()
            .count(),
        1,
        "2026/ だけ"
    );
    assert_eq!(
        fs::read_dir(f.root().join("kb/tech/notes/2026"))
            .unwrap()
            .count(),
        0
    );

    let with_slug = [&args[..], &["--slug", "pricing-review"]].concat();
    assert!(f.ika_with(&with_slug, Some("")).ok);
    assert!(f.exists("kb/tech/notes/2026/2026-10-14-pricing-review.md"));

    let japanese_slug = [&args[..], &["--slug", "料金"]].concat();
    assert!(
        !f.ika_with(&japanese_slug, Some("")).ok,
        "日本語のスラッグは英字にならないので失敗"
    );
}

#[test]
fn mv_fix_links_rewrites_referrers_and_own_links() {
    let f = Fixture::new("mv-fix");
    let from = "kb/tech/notes/2026/2026-10-14-a.md";
    let to = "kb/tech/insights/a.md";
    f.write(
        from,
        &NOTE.replace(
            "本文",
            "[i](../../insights/i.md) と [自分](2026-10-14-a.md) と [無い](missing.md)",
        ),
    );
    f.write("kb/tech/insights/i.md", NOTE);
    f.write(
        "kb/tech/insights/ref.md",
        &NOTE.replace(
            "本文",
            "[a](../notes/2026/2026-10-14-a.md#sec) と `[code](../notes/2026/2026-10-14-a.md)` と [外](https://x.test/2026-10-14-a.md)",
        ),
    );

    let run = f.ika(&["kb", "mv", from, to, "--fix-links"]);
    assert!(run.ok, "{}", run.err);
    assert!(run.out.contains("リンクを書き換えました"), "{}", run.out);

    let referrer = f.read("kb/tech/insights/ref.md");
    assert!(referrer.contains("[a](a.md#sec)"), "{referrer}");
    assert!(
        referrer.contains("`[code](../notes/2026/2026-10-14-a.md)`"),
        "コードは触らない"
    );
    assert!(
        referrer.contains("(https://x.test/2026-10-14-a.md)"),
        "外部URLは触らない"
    );

    let moved = f.read(to);
    assert!(moved.contains("[i](i.md)"), "{moved}");
    assert!(moved.contains("[自分](a.md)"), "{moved}");
    assert!(
        moved.contains("[無い](missing.md)"),
        "移動前から切れているリンクは直さない"
    );
}

#[test]
fn mv_without_fix_links_suggests_the_flag() {
    let f = Fixture::new("mv-hint");
    f.write("kb/tech/notes/2026/2026-10-14-a.md", NOTE);
    f.write(
        "kb/tech/insights/i.md",
        &NOTE.replace("本文", "[a](../notes/2026/2026-10-14-a.md)"),
    );
    let run = f.ika(&[
        "kb",
        "mv",
        "kb/tech/notes/2026/2026-10-14-a.md",
        "kb/tech/insights/a.md",
    ]);
    assert!(run.out.contains("--fix-links"), "{}", run.out);
    assert!(
        f.read("kb/tech/insights/i.md")
            .contains("../notes/2026/2026-10-14-a.md"),
        "書き換えない"
    );
}

#[test]
fn project_show_lists_related_notes_newest_first() {
    let f = Fixture::new("show");
    let doc = |ty: &str, date: &str, projects: &str, extra: &str| {
        format!(
            "---\ntitle: {ty} {date}\ntype: {ty}\ndate: {date}\nprojects: {projects}\ntags: []\n{extra}---\n\n本文\n"
        )
    };
    f.write(
        "kb/tech/notes/2026/2026-10-01-old.md",
        &doc("note", "2026-10-01", "[foo]", ""),
    );
    f.write(
        "kb/tech/notes/2026/2026-10-10-new.md",
        &doc("note", "2026-10-10", "[foo]", ""),
    );
    f.write(
        "kb/tech/notes/2026/2026-10-11-other.md",
        &doc("note", "2026-10-11", "[]", ""),
    );
    f.write(
        "kb/tech/insights/i.md",
        &doc("insight", "2026-10-05", "[foo]", ""),
    );
    fs::create_dir_all(f.root().join("kb/tech/decisions")).unwrap();
    f.write(
        "kb/tech/decisions/2026-10-02-d.md",
        &doc("decision", "2026-10-02", "[foo]", "status: accepted\n"),
    );

    let run = f.ika(&["project", "show", "foo"]);
    assert!(run.ok, "{}", run.err);
    let paths: Vec<_> = run
        .out
        .lines()
        .filter_map(|l| l.trim().strip_prefix("kb/"))
        .collect();
    assert_eq!(
        paths,
        [
            "tech/decisions/2026-10-02-d.md",
            "tech/insights/i.md",
            "tech/notes/2026/2026-10-10-new.md",
            "tech/notes/2026/2026-10-01-old.md",
        ],
        "{}",
        run.out
    );
    assert!(run.out.contains("[decision accepted]"));
    assert!(!run.out.contains("other"), "無関係なノートは出さない");
    assert!(
        !run.out.contains("# foo"),
        "--readme なしでは本文を出さない"
    );

    let with_readme = f.ika(&["project", "show", "foo", "--readme"]);
    assert!(with_readme.out.contains("# foo"));
    assert!(!f.ika(&["project", "show", "nope"]).ok);
}

#[test]
fn nothing_works_without_ika_home() {
    let f = Fixture::new("no-home");
    for args in [
        vec!["kb", "ls"],
        vec!["project", "list"],
        vec!["kb", "search", "x"],
        vec!["serve", "--port", "0"],
        vec!["check"],
    ] {
        let run = f.run(&args, None, None);
        assert!(!run.ok, "{args:?}");
        assert!(
            run.err.contains("IKA_HOME が設定されていません"),
            "{}",
            run.err
        );
    }
    // ホームの下の決まった場所にフォールバックしない
    fs::create_dir_all(f.dir.join(".ika/data")).unwrap();
    fs::write(
        f.dir.join(".ika/data/projects.csv"),
        "name,path,url,status,summary\n",
    )
    .unwrap();
    assert!(!f.run(&["project", "list"], None, None).ok);
}

#[test]
fn commands_fail_clearly_when_the_kb_is_not_initialized() {
    let f = Fixture::bare("uninit");
    let run = f.ika(&["kb", "ls"]);
    assert!(!run.ok);
    assert!(run.err.contains("ika init"), "{}", run.err);
}

#[test]
fn init_creates_an_empty_kb_with_local_git_history() {
    let f = Fixture::bare("init");
    let run = f.ika(&["init"]);
    assert!(run.ok, "{}", run.err);
    assert_eq!(
        f.read("data/projects.csv"),
        "name,path,url,status,summary\n"
    );
    assert_eq!(f.read("data/domains.csv"), "name,summary\n");
    assert!(f.exists("kb/projects"));
    assert!(f.exists(".git"));
    assert_eq!(f.git(&["log", "--format=%s"]).trim(), "ika init");

    assert!(!f.ika(&["init"]).ok, "2回目は失敗する");
    assert!(f.ika(&["project", "list"]).ok);
}

#[test]
fn init_import_copies_kb_without_the_format_readme_or_symlinks() {
    let f = Fixture::bare("import");
    let repo = f.dir.join("old-repo");
    fs::create_dir_all(repo.join("data")).unwrap();
    fs::create_dir_all(repo.join("kb/tech/notes/2026")).unwrap();
    fs::write(
        repo.join("data/projects.csv"),
        "name,path,url,status,summary\nfoo,/path/to/foo,,active,Foo\n",
    )
    .unwrap();
    fs::write(repo.join("data/domains.csv"), "name,summary\ntech,t\n").unwrap();
    fs::write(repo.join("kb/README.md"), "# 書式\n").unwrap();
    fs::write(repo.join("kb/tech/notes/2026/2026-10-14-a.md"), NOTE).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("/etc/passwd", repo.join("kb/tech/link.md")).unwrap();

    let run = f.ika(&["init", "--import", repo.to_str().unwrap()]);
    assert!(run.ok, "{}", run.err);
    assert_eq!(f.read("kb/tech/notes/2026/2026-10-14-a.md"), NOTE);
    assert!(!f.exists("kb/README.md"));
    assert!(!f.exists("kb/tech/link.md"));
    assert!(f.read("data/projects.csv").contains("foo"));
    assert_eq!(
        f.git(&["status", "--porcelain"]),
        "",
        "取り込んだ内容は最初のコミットに入る"
    );
    assert!(
        f.ika(&["kb", "search", "本文", "-l"])
            .out
            .contains("2026-10-14-a.md")
    );
}

#[test]
fn every_write_is_committed_to_the_local_history() {
    let f = Fixture::bare("history");
    assert!(f.ika(&["init"]).ok);
    assert!(f.ika(&["domain", "add", "tech", "--summary", "t"]).ok);
    assert!(f.ika(&["project", "add", "foo", "--summary", "Foo"]).ok);
    let new = f.ika_with(
        &[
            "new",
            "note",
            "A",
            "--domain",
            "tech",
            "--project",
            "foo",
            "--date",
            "2026-10-14",
            "--stdin",
        ],
        Some("本文\n"),
    );
    assert!(new.ok, "{}", new.err);
    let path = "kb/tech/notes/2026/2026-10-14-a.md";
    assert!(
        f.ika(&["kb", "edit", path, "--old", "本文", "--new", "編集"])
            .ok
    );
    assert!(f.ika_with(&["kb", "append", path], Some("追記\n")).ok);
    assert!(
        f.ika(&["kb", "mv", path, "kb/tech/notes/2026/2026-10-14-b.md"])
            .ok
    );
    assert!(f.ika(&["project", "set", "foo", "--status", "paused"]).ok);
    assert!(
        f.ika(&["kb", "rm", "kb/tech/notes/2026/2026-10-14-b.md"])
            .ok
    );

    let log = f.git(&["log", "--format=%s", "--reverse"]);
    let subjects: Vec<_> = log.lines().collect();
    assert_eq!(
        subjects,
        [
            "ika init",
            "domain add tech",
            "project add foo",
            "new note: kb/tech/notes/2026/2026-10-14-a.md",
            "kb edit kb/tech/notes/2026/2026-10-14-a.md",
            "kb append kb/tech/notes/2026/2026-10-14-a.md",
            "kb mv kb/tech/notes/2026/2026-10-14-a.md kb/tech/notes/2026/2026-10-14-b.md",
            "project set foo",
            "kb rm kb/tech/notes/2026/2026-10-14-b.md",
        ]
    );
    assert_eq!(
        f.git(&["status", "--porcelain"]),
        "",
        "未コミットの変更は残らない"
    );
}

#[test]
fn failed_commands_leave_no_commit() {
    let f = Fixture::bare("no-commit");
    assert!(f.ika(&["init"]).ok);
    assert!(!f.ika(&["kb", "rm", "kb/tech/none.md"]).ok);
    assert!(!f.ika_with(&["kb", "write", "data/x.md"], Some("x\n")).ok);
    assert_eq!(f.git(&["log", "--format=%s"]).trim(), "ika init");
}

#[test]
fn check_works_outside_the_repository() {
    let f = Fixture::new("check");
    f.write("kb/tech/notes/2026/2026-10-14-a.md", NOTE);
    let run = f.ika(&["check"]);
    assert!(run.ok, "{}", run.out);
    assert!(run.out.contains("OK"));
    assert!(
        run.out.contains("プラグインの検証は省きました"),
        "{}",
        run.out
    );
}

#[test]
fn grep_passes_arguments_to_ripgrep_and_stays_inside_kb() {
    if Command::new("rg").arg("--version").output().is_err() {
        return;
    }
    let f = Fixture::new("grep");
    f.write(
        "kb/tech/notes/2026/2026-10-14-a.md",
        &NOTE.replace("本文", "Review の話"),
    );
    f.write("data/secret.csv", "Review,secret\n");
    let hit = f.ika(&["kb", "grep", "-n", "-i", "review|レビュー", "kb/tech"]);
    assert!(hit.ok, "{}", hit.err);
    assert!(
        hit.out.contains("kb/tech/notes/2026/2026-10-14-a.md:"),
        "{}",
        hit.out
    );
    let all = f.ika(&["kb", "grep", "-l", "Review"]);
    assert_eq!(
        all.out.trim(),
        "kb/tech/notes/2026/2026-10-14-a.md",
        "kb/ の外は検索しない"
    );
    let none = f.ika(&["kb", "grep", "存在しない語"]);
    assert!(!none.ok && none.out.is_empty());
    assert!(!f.ika(&["kb", "grep", "--pre", "cat", "x"]).ok);
    let outside = f.dir.join("outside.txt");
    fs::write(&outside, "Review\n").unwrap();
    let escaped = f.ika(&["kb", "grep", "Review", outside.to_str().unwrap()]);
    assert!(!escaped.ok && escaped.out.is_empty(), "{}", escaped.out);
    assert!(!f.ika(&["kb", "grep", "Review", "../data"]).ok);
}

#[test]
fn search_any_matches_either_term() {
    let f = Fixture::new("any");
    f.write(
        "kb/tech/notes/2026/2026-10-14-a.md",
        &NOTE.replace("本文", "Review の話"),
    );
    f.write(
        "kb/tech/notes/2026/2026-10-15-b.md",
        &NOTE.replace("本文", "レビューの話"),
    );
    let all = f.ika(&["kb", "search", "Review", "レビュー", "-l"]);
    assert!(all.out.is_empty(), "既定は AND: {}", all.out);
    let any = f.ika(&["kb", "search", "Review", "レビュー", "--any", "-l"]);
    assert_eq!(any.out.lines().count(), 2, "{}", any.out);
}

#[test]
fn search_orders_results_by_kind_then_newest() {
    let f = Fixture::new("rank");
    let doc = |ty: &str, date: &str, extra: &str| {
        format!(
            "---\ntitle: x\ntype: {ty}\ndate: {date}\nprojects: []\ntags: []\n{extra}---\n\nfoo の話\n"
        )
    };
    f.write(
        "kb/tech/notes/2026/2026-10-01-old.md",
        &doc("note", "2026-10-01", ""),
    );
    f.write(
        "kb/tech/notes/2026/2026-10-09-new.md",
        &doc("note", "2026-10-09", ""),
    );
    f.write("kb/tech/insights/i.md", &doc("insight", "2026-10-02", ""));
    f.write(
        "kb/tech/decisions/2026-10-03-ok.md",
        &doc("decision", "2026-10-03", "status: accepted\n"),
    );
    f.write(
        "kb/tech/decisions/2026-10-10-old-way.md",
        &doc("decision", "2026-10-10", "status: superseded\n"),
    );

    let run = f.ika(&["kb", "search", "foo", "-l"]);
    let paths: Vec<_> = run.out.lines().collect();
    assert_eq!(
        paths,
        [
            "kb/tech/decisions/2026-10-03-ok.md",
            "kb/tech/insights/i.md",
            "kb/projects/foo/README.md",
            "kb/tech/notes/2026/2026-10-09-new.md",
            "kb/tech/notes/2026/2026-10-01-old.md",
            "kb/tech/decisions/2026-10-10-old-way.md",
        ],
        "{}",
        run.out
    );

    let detailed = f.ika(&["kb", "search", "foo"]).out;
    assert!(
        detailed
            .starts_with("kb/tech/decisions/2026-10-03-ok.md  [decision accepted] 2026-10-03\n  "),
        "{detailed}"
    );
    assert!(detailed.contains("[insight] 2026-10-02"));
}

impl Fixture {
    /// `ika export` が読む最小の ika リポジトリ（AGENTS.md・プラグイン・書式）。ここで実行する
    fn repo(&self) -> PathBuf {
        let repo = self.dir.join("repo");
        fs::create_dir_all(repo.join("plugins/ika/skills/demo")).unwrap();
        fs::create_dir_all(repo.join("docs")).unwrap();
        fs::write(repo.join("AGENTS.md"), "# 指示\n").unwrap();
        fs::write(repo.join("docs/kb-format.md"), "# 書式\n").unwrap();
        fs::write(
            repo.join("plugins/ika/skills/demo/SKILL.md"),
            "---\nname: demo\ndescription: デモ\n---\n\n本文\n",
        )
        .unwrap();
        repo
    }

    fn export(&self, args: &[&str]) -> Run {
        let repo = self.repo();
        let mut full = vec!["export"];
        full.extend_from_slice(args);
        self.run_in(&repo, &full, None, Some(self.root()))
    }

    /// tech と business の2領域に、ノートとプロダクトがある KB
    fn kb_with_two_domains(&self) {
        self.write("data/domains.csv", "name,summary\ntech,t\nbusiness,b\n");
        self.write(
            "kb/tech/notes/2026/2026-10-14-a.md",
            &NOTE.replace("本文", "技術の話"),
        );
        self.write(
            "kb/business/notes/2026/2026-10-14-b.md",
            &NOTE.replace("本文", "価格の話"),
        );
    }
}

fn dist(f: &Fixture, target: &str) -> PathBuf {
    f.dir.join("repo/dist").join(target)
}

#[test]
fn export_claude_does_not_include_the_kb_by_default() {
    let f = Fixture::new("export-claude");
    f.kb_with_two_domains();
    let out = dist(&f, "claude");
    // 前回の書き出しが残っていても消える
    fs::create_dir_all(&out).unwrap();
    fs::write(out.join("knowledge-tech.md"), "古い").unwrap();
    fs::write(out.join("projects.csv"), "古い").unwrap();

    let run = f.export(&["claude"]);
    assert!(run.ok, "{}", run.err);
    assert!(out.join("ika.zip").is_file());
    let names: Vec<_> = fs::read_dir(&out)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["ika.zip"], "{names:?}");
}

#[test]
fn export_claude_with_kb_writes_a_read_only_snapshot() {
    let f = Fixture::new("export-claude-kb");
    f.kb_with_two_domains();
    let run = f.export(&["claude", "--with-kb"]);
    assert!(run.ok, "{}", run.err);
    assert!(run.out.contains("注意: KB の内容"), "{}", run.out);

    let out = dist(&f, "claude");
    let tech = fs::read_to_string(out.join("knowledge-tech.md")).unwrap();
    assert!(tech.contains("<!-- path: kb/tech/notes/2026/2026-10-14-a.md -->"));
    assert!(tech.contains("技術の話"));
    assert!(!tech.contains("価格の話"));
    let business = fs::read_to_string(out.join("knowledge-business.md")).unwrap();
    assert!(business.contains("価格の話"));
    assert!(
        fs::read_to_string(out.join("knowledge-projects.md"))
            .unwrap()
            .contains("# foo")
    );
    assert!(
        fs::read_to_string(out.join("projects.csv"))
            .unwrap()
            .contains("foo")
    );
    let instructions = fs::read_to_string(out.join("instructions.md")).unwrap();
    assert!(instructions.contains("# 指示") && instructions.contains("claude.ai での使い方"));
}

#[test]
fn export_claude_with_kb_can_limit_the_scope() {
    let f = Fixture::new("export-claude-scope");
    f.kb_with_two_domains();
    let run = f.export(&["claude", "--with-kb", "--domain", "tech", "--no-projects"]);
    assert!(run.ok, "{}", run.err);

    let out = dist(&f, "claude");
    assert!(out.join("knowledge-tech.md").is_file());
    assert!(!out.join("knowledge-business.md").exists());
    assert!(!out.join("knowledge-projects.md").exists());
    assert!(!out.join("projects.csv").exists());
    let domains = fs::read_to_string(out.join("domains.csv")).unwrap();
    assert!(
        domains.contains("tech") && !domains.contains("business"),
        "{domains}"
    );

    // 範囲を広げて出し直すと入れ替わり、狭めて出し直すと古いものは消える
    assert!(f.export(&["claude", "--with-kb"]).ok);
    assert!(out.join("knowledge-business.md").is_file());
    assert!(f.export(&["claude"]).ok);
    assert!(!out.join("knowledge-business.md").exists());
}

#[test]
fn export_claude_rejects_scope_flags_without_with_kb_and_unknown_domains() {
    let f = Fixture::new("export-claude-flags");
    f.kb_with_two_domains();
    let run = f.export(&["claude", "--domain", "tech"]);
    assert!(!run.ok);
    assert!(run.err.contains("--with-kb"), "{}", run.err);

    let unknown = f.export(&["claude", "--with-kb", "--domain", "nope"]);
    assert!(!unknown.ok);
    assert!(unknown.err.contains("未登録の領域"), "{}", unknown.err);
}

#[test]
fn export_chatgpt_honors_the_same_scope() {
    let f = Fixture::new("export-chatgpt");
    f.kb_with_two_domains();
    let run = f.export(&["chatgpt", "--domain", "business", "--no-projects"]);
    assert!(run.ok, "{}", run.err);

    let out = dist(&f, "chatgpt");
    assert!(out.join("instructions.md").is_file() && out.join("skills.md").is_file());
    assert!(out.join("knowledge-business.md").is_file());
    assert!(!out.join("knowledge-tech.md").exists());
    assert!(!out.join("knowledge-projects.md").exists());
}

// ---- ika serve ----

struct Server {
    child: std::process::Child,
    port: u16,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Reply {
    status: u16,
    headers: String,
    body: String,
}

impl Fixture {
    /// `ika serve --port 0` を起動し、表示された URL からポートを読む
    fn serve(&self) -> Server {
        use std::io::{BufRead, BufReader};
        let mut child = Command::new(env!("CARGO_BIN_EXE_ika"))
            .args(["serve", "--port", "0"])
            .env("IKA_HOME", self.root())
            .env("HOME", &self.dir)
            .current_dir(self.dir.join("work"))
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut first = String::new();
        // stdout は Child に持たせたままにする。閉じると、サーバーが続きの行を出すときに BrokenPipe で終わる
        BufReader::new(child.stdout.as_mut().unwrap())
            .read_line(&mut first)
            .unwrap();
        let port = first
            .trim()
            .strip_prefix("http://127.0.0.1:")
            .and_then(|rest| rest.trim_end_matches('/').parse().ok())
            .unwrap_or_else(|| panic!("URL が読めません: {first:?}"));
        Server { child, port }
    }
}

fn raw_request(port: u16, request: &str) -> Reply {
    use std::io::Read;
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).unwrap();
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let (headers, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    Reply {
        status: headers
            .split(' ')
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap(),
        headers: headers.to_string(),
        body: body.to_string(),
    }
}

fn get(server: &Server, path: &str) -> Reply {
    raw_request(
        server.port,
        &format!(
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n\r\n",
            server.port
        ),
    )
}

#[test]
fn serve_shows_the_index_and_renders_markdown_pages() {
    let f = Fixture::new("serve-pages");
    f.write(
        "kb/tech/notes/2026/2026-10-14-a.md",
        &NOTE.replace(
            "本文",
            "## 要点\n\n- 箇条書き\n\n| a | b |\n|---|---|\n| 1 | 2 |\n",
        ),
    );
    f.write(
        "kb/tech/insights/i.md",
        &NOTE.replace("本文", "[根拠](../notes/2026/2026-10-14-a.md)"),
    );
    let server = f.serve();

    let index = get(&server, "/");
    assert_eq!(index.status, 200);
    assert!(
        index.body.contains("href=\"/project/foo\""),
        "{}",
        index.body
    );
    assert!(index.body.contains("href=\"/kb/tech/\""));
    assert!(
        index
            .body
            .contains("href=\"/kb/tech/notes/2026/2026-10-14-a.md\"")
    );

    let page = get(&server, "/kb/tech/notes/2026/2026-10-14-a.md");
    assert_eq!(page.status, 200);
    assert!(page.headers.contains("text/html; charset=utf-8"));
    assert!(page.body.contains("<h1>n</h1>"), "{}", page.body);
    assert!(
        page.body.contains("<h2 id=\"要点\">要点</h2>") && page.body.contains("<li>箇条書き</li>")
    );
    assert!(page.body.contains("<table>"));
    assert!(
        page.body.contains("href=\"/project/foo\"") && page.body.contains("href=\"/tags/auth\"")
    );
    assert!(
        !page.body.contains("title: n"),
        "フロントマターは本文に出さない"
    );
    assert!(
        page.body.contains("このページへのリンク") && page.body.contains("/kb/tech/insights/i.md")
    );
}

#[test]
fn serve_lists_tags_and_shows_pages_by_tag() {
    let f = Fixture::new("serve-tags");
    f.write("kb/tech/notes/2026/2026-10-14-a.md", NOTE);
    f.write(
        "kb/tech/insights/i.md",
        &NOTE
            .replace("type: note", "type: insight")
            .replace("tags: [auth]", "tags: [auth, 日本 語]"),
    );
    f.write(
        "kb/tech/notes/2026/2026-10-15-b.md",
        &NOTE.replace("tags: [auth]", "tags: [other]"),
    );
    let server = f.serve();

    let index = get(&server, "/tags");
    assert_eq!(index.status, 200);
    assert!(
        index.body.contains("href=\"/tags/auth\"") && index.body.contains("href=\"/tags/other\""),
        "{}",
        index.body
    );
    assert!(get(&server, "/").body.contains("href=\"/tags/auth\""));

    let page = get(&server, "/tags/auth");
    assert_eq!(page.status, 200);
    assert!(page.body.contains("2 件"), "{}", page.body);
    assert!(
        page.body.contains("/kb/tech/notes/2026/2026-10-14-a.md")
            && page.body.contains("/kb/tech/insights/i.md")
            && !page.body.contains("2026-10-15-b.md"),
        "{}",
        page.body
    );
    // 知見はノートより先、一緒に付いているタグも出す
    assert!(page.body.find("知見").unwrap() < page.body.find("ノート").unwrap());
    assert!(
        page.body
            .contains("href=\"/tags/%E6%97%A5%E6%9C%AC%20%E8%AA%9E\"")
    );
    assert_eq!(
        get(&server, "/tags/%E6%97%A5%E6%9C%AC%20%E8%AA%9E").status,
        200
    );
    assert_eq!(get(&server, "/tags/none").status, 404);
}

#[test]
fn serve_resolves_relative_links_between_notes_in_the_browser() {
    // 相対リンクは元のファイルの場所から解決する。README はプロダクトのページ（/project/foo）にも
    // 埋め込むので、ページの URL を基準にすると切れる
    let f = Fixture::new("serve-links");
    f.write("kb/tech/notes/2026/2026-10-14-a.md", NOTE);
    f.write(
        "kb/tech/insights/i.md",
        &NOTE.replace("本文", "[根拠](../notes/2026/2026-10-14-a.md)"),
    );
    f.write(
        "kb/projects/foo/README.md",
        "---\ntitle: foo\ntype: project\nprojects: [foo]\n---\n\n[メモ](../../tech/notes/2026/2026-10-14-a.md) [要件](specs/s.md)\n",
    );
    f.write("kb/projects/foo/specs/s.md", NOTE);
    let server = f.serve();
    let page = get(&server, "/kb/tech/insights/i.md");
    assert!(
        page.body
            .contains("href=\"/kb/tech/notes/2026/2026-10-14-a.md\""),
        "{}",
        page.body
    );
    for url in ["/project/foo", "/kb/projects/foo/README.md"] {
        let page = get(&server, url);
        for href in [
            "/kb/tech/notes/2026/2026-10-14-a.md",
            "/kb/projects/foo/specs/s.md",
        ] {
            assert!(
                page.body.contains(&format!("href=\"{href}\"")),
                "{url}: {}",
                page.body
            );
            assert_eq!(get(&server, href).status, 200, "{href}");
        }
    }
}

#[test]
fn serve_never_emits_raw_html_or_script_urls_from_notes() {
    let f = Fixture::new("serve-xss");
    f.write(
        "kb/tech/notes/2026/2026-10-14-a.md",
        &NOTE
            .replace("title: n", "title: <img src=x onerror=alert(1)>")
            .replace("本文", "<script>alert(1)</script>\n\n[x](javascript:alert(1))\n\n<b onclick=\"x()\">bold</b>"),
    );
    let server = f.serve();
    let page = get(&server, "/kb/tech/notes/2026/2026-10-14-a.md");
    assert!(
        !page.body.contains("<script>") && !page.body.contains("<img src=x"),
        "{}",
        page.body
    );
    assert!(!page.body.contains("javascript:"), "{}", page.body);
    assert!(page.body.contains("&lt;script&gt;"));
    assert!(
        page.headers
            .contains("Content-Security-Policy: default-src 'none'"),
        "{}",
        page.headers
    );
    assert!(page.headers.contains("Referrer-Policy: no-referrer"));
}

#[test]
fn serve_lists_directories_newest_first() {
    let f = Fixture::new("serve-dirs");
    f.write("kb/tech/notes/2026/2026-10-01-old.md", NOTE);
    f.write("kb/tech/notes/2026/2026-10-09-new.md", NOTE);
    let server = f.serve();

    let notes = get(&server, "/kb/tech/notes/");
    assert_eq!(notes.status, 200);
    assert!(notes.body.contains("href=\"/kb/tech/notes/2026/\""));
    let year = get(&server, "/kb/tech/notes/2026");
    let new = year.body.find("2026-10-09-new.md").unwrap();
    let old = year.body.find("2026-10-01-old.md").unwrap();
    assert!(new < old, "新しい順");
    assert_eq!(get(&server, "/kb").status, 200);
}

#[test]
fn serve_shows_projects_and_searches_in_trust_order() {
    let f = Fixture::new("serve-search");
    f.write(
        "kb/tech/notes/2026/2026-10-14-a.md",
        &NOTE.replace("本文", "foo の話"),
    );
    f.write(
        "kb/tech/decisions/2026-10-15-d.md",
        "---\ntitle: 決定\ntype: decision\ndate: 2026-10-15\nprojects: [foo]\ntags: []\nstatus: accepted\n---\n\nfoo の決定\n",
    );
    let server = f.serve();

    let project = get(&server, "/project/foo");
    assert_eq!(project.status, 200);
    assert!(project.body.contains("関連（2 件）"), "{}", project.body);
    assert_eq!(
        (
            project.body.matches("<h1>foo</h1>").count(),
            project.body.matches("<h1 id=\"foo\">foo</h1>").count()
        ),
        (1, 1),
        "見出しと、描画された README"
    );
    assert!(project.body.contains("2026-10-14-a.md"));
    assert_eq!(get(&server, "/project/nope").status, 404);

    // 既定は全文検索（関連度順）。「文字列一致」は従来の検索（信頼度順）
    let found = get(&server, "/search?q=foo");
    assert_eq!(found.status, 200);
    assert!(found.body.contains("関連の強い順"), "{}", found.body);
    assert!(found.body.contains("2026-10-15-d.md") && found.body.contains("2026-10-14-a.md"));
    assert!(found.body.contains("decision accepted"));
    let exact = get(&server, "/search?q=foo&exact=1");
    assert!(
        exact.body.contains("文字列一致・信頼度順"),
        "{}",
        exact.body
    );
    let decision = exact.body.find("2026-10-15-d.md").unwrap();
    let note = exact.body.find("2026-10-14-a.md").unwrap();
    assert!(decision < note, "採用済みの決定が先");
    // 文で引ける（助詞を除いた「決定」で当たる）
    assert!(
        get(&server, "/search?q=foo%E3%81%AE%E6%B1%BA%E5%AE%9A%E3%81%AF")
            .body
            .contains("2026-10-15-d.md")
    );
    let tagged = get(&server, "/search?tag=auth&domain=tech");
    assert!(tagged.body.contains("1 件"), "{}", tagged.body);
    assert!(tagged.body.contains("2026-10-14-a.md"));
    assert!(
        get(&server, "/search")
            .body
            .contains("検索語か絞り込みを指定してください")
    );
    // 日本語のクエリ（パーセントエンコード）
    assert!(
        get(&server, "/search?q=%E6%B1%BA%E5%AE%9A")
            .body
            .contains("2026-10-15-d.md")
    );
}

#[test]
fn serve_reads_files_fresh_on_every_request() {
    let f = Fixture::new("serve-live");
    let server = f.serve();
    assert_eq!(
        get(&server, "/kb/tech/notes/2026/2026-10-14-a.md").status,
        404
    );
    f.write("kb/tech/notes/2026/2026-10-14-a.md", NOTE);
    assert_eq!(
        get(&server, "/kb/tech/notes/2026/2026-10-14-a.md").status,
        200
    );
}

#[test]
fn serve_is_read_only_and_only_for_localhost_names() {
    let f = Fixture::new("serve-security");
    let server = f.serve();
    let port = server.port;

    let post = raw_request(
        port,
        &format!("POST / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: 0\r\n\r\n"),
    );
    assert_eq!(post.status, 405);
    assert!(post.headers.contains("Allow: GET, HEAD"));

    for host in [
        "evil.example",
        &format!("evil.example:{port}"),
        &format!("127.0.0.1.evil.example:{port}"),
    ] {
        let reply = raw_request(port, &format!("GET / HTTP/1.1\r\nHost: {host}\r\n\r\n"));
        assert_eq!(reply.status, 403, "{host}");
    }
    let no_host = raw_request(port, "GET / HTTP/1.0\r\n\r\n");
    assert_eq!(no_host.status, 403);
    let localhost = raw_request(
        port,
        &format!("GET / HTTP/1.1\r\nHost: LocalHost:{port}\r\n\r\n"),
    );
    assert_eq!(localhost.status, 200);

    let head = raw_request(
        port,
        &format!("HEAD / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"),
    );
    assert_eq!(head.status, 200);
    assert!(head.headers.contains("Content-Length:") && head.body.is_empty());
    assert_eq!(raw_request(port, "garbage\r\n\r\n").status, 400);
}

#[test]
fn serve_cannot_read_outside_kb() {
    let f = Fixture::new("serve-escape");
    f.write("kb/tech/secret.txt", "secret");
    fs::write(f.dir.join("work/outside.md"), "# outside").unwrap();
    let server = f.serve();

    for path in [
        "/kb/../data/projects.csv",
        "/kb/%2e%2e/data/projects.csv",
        "/kb/tech/%2e%2e/%2e%2e/data/projects.csv",
        "/kb/..%2fdata/projects.csv",
        "/kb//etc/passwd",
        "/kb/tech/%00.md",
    ] {
        let reply = get(&server, path);
        assert!(
            matches!(reply.status, 400 | 404),
            "{path} -> {}",
            reply.status
        );
        assert!(!reply.body.contains("name,path"), "{path}");
        assert!(!reply.body.contains("root:"), "{path}");
    }
    assert_eq!(get(&server, "/data/projects.csv").status, 404);
    assert_eq!(
        get(&server, "/kb/tech/secret.txt").status,
        404,
        "Markdown と画像以外は返さない"
    );
    assert_eq!(get(&server, "/etc/passwd").status, 404);

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink(
            f.dir.join("work/outside.md"),
            f.root().join("kb/tech/link.md"),
        )
        .unwrap();
        symlink(f.dir.join("work"), f.root().join("kb/tech/linkdir")).unwrap();
        for path in [
            "/kb/tech/link.md",
            "/kb/tech/linkdir/outside.md",
            "/kb/tech/linkdir/",
        ] {
            let reply = get(&server, path);
            assert_eq!(reply.status, 403, "{path}");
            assert!(!reply.body.contains("# outside"), "{path}");
        }
    }
}

#[test]
fn serve_serves_images_with_their_type() {
    let f = Fixture::new("serve-image");
    fs::write(
        f.root().join("kb/projects/foo/a.png"),
        [0x89, b'P', b'N', b'G'],
    )
    .unwrap();
    fs::write(
        f.root().join("kb/projects/foo/b.svg"),
        "<svg onload=alert(1)/>",
    )
    .unwrap();
    let server = f.serve();
    let png = get(&server, "/kb/projects/foo/a.png");
    assert_eq!(png.status, 200);
    assert!(png.headers.contains("Content-Type: image/png"));
    assert!(png.headers.contains("X-Content-Type-Options: nosniff"));
    assert_eq!(
        get(&server, "/kb/projects/foo/b.svg").status,
        404,
        "SVG はスクリプトを含みうるので返さない"
    );
}

#[test]
fn serve_loads_the_japanese_web_font_without_blocking_the_page() {
    let f = Fixture::new("serve-font");
    let server = f.serve();
    let page = get(&server, "/");

    // CDN の Web フォントは本文のあとに置く（オフラインでも本文の表示が止まらない）
    let link = page
        .body
        .find("https://fonts.googleapis.com/css2?family=Noto+Sans+JP")
        .unwrap_or_else(|| panic!("{}", page.body));
    assert!(link > page.body.find("</main>").unwrap());
    assert!(page.body.contains("display=swap"));

    // 読み込めない場合は、システムの日本語フォントと sans-serif に落ちる
    let family = page
        .body
        .split("font:17px/1.9")
        .nth(1)
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    assert!(
        family.contains("Hiragino") && family.ends_with("sans-serif"),
        "{family}"
    );

    // CSP は、フォントの取得先だけを足す（スクリプトは不可のまま）
    assert!(
        page.headers
            .contains("style-src 'unsafe-inline' https://fonts.googleapis.com;"),
        "{}",
        page.headers
    );
    assert!(page.headers.contains("font-src https://fonts.gstatic.com;"));
    assert!(page.headers.contains("default-src 'none'") && !page.headers.contains("script-src"));
}

// ---- ika kb orphans ----

#[test]
fn orphans_lists_notes_not_linked_from_insights_decisions_or_project_docs() {
    let f = Fixture::new("orphans");
    let note = |slug: &str, projects: &str| {
        f.write(
            &format!("kb/tech/notes/2026/2026-10-14-{slug}.md"),
            &NOTE.replace("projects: [foo]", &format!("projects: {projects}")),
        );
    };
    for slug in ["a", "b", "c", "d", "e"] {
        note(slug, "[foo]");
    }
    note("solo", "[]");
    note("two", "[foo, bar]");

    let link = |slug: &str| format!("[{slug}](../notes/2026/2026-10-14-{slug}.md)");
    // insight・decision・README 以外のプロダクト資料からのリンクは「整理済み」
    f.write(
        "kb/tech/insights/i.md",
        &NOTE
            .replace("本文", &link("a"))
            .replace("type: note", "type: insight"),
    );
    f.write(
        "kb/tech/decisions/2026-10-15-d.md",
        &NOTE
            .replace("本文", "[b](../notes/2026/2026-10-14-b.md#sec)")
            .replace("type: note", "type: decision\nstatus: accepted"),
    );
    f.write(
        "kb/projects/foo/design.md",
        &NOTE.replace("本文", "[c](../../tech/notes/2026/2026-10-14-c.md)"),
    );
    // README の「関連」は索引なので、整理済みとは数えない
    fs::write(
        f.root().join("kb/projects/foo/README.md"),
        "---\ntitle: foo\ntype: project\ndate: 2026-10-14\nprojects: [foo]\ntags: []\n---\n\n[d](../../tech/notes/2026/2026-10-14-d.md)\n",
    )
    .unwrap();

    let run = f.ika(&["kb", "orphans"]);
    assert!(run.ok, "{}", run.err);
    let out = run.out;
    // foo: d, e, two が未整理（a・b・c は整理済み）
    assert!(out.contains("foo  3 件  ← 要整理（3 件以上）"), "{out}");
    assert!(out.contains("2026-10-14-d.md") && out.contains("2026-10-14-e.md"));
    assert!(
        !out.contains("2026-10-14-a.md")
            && !out.contains("2026-10-14-b.md")
            && !out.contains("2026-10-14-c.md"),
        "{out}"
    );
    // 複数プロダクトのノートはそれぞれに出る。プロダクトなしも別枠
    assert!(out.contains("bar  1 件\n"), "{out}");
    assert!(!out.contains("bar  1 件  ←"));
    assert!(out.contains("（プロダクトなし）  1 件"), "{out}");
    // 多い順
    assert!(out.find("foo  3 件").unwrap() < out.find("bar  1 件").unwrap());

    let only = f.ika(&["kb", "orphans", "-p", "bar"]).out;
    assert!(
        only.contains("bar  1 件") && !only.contains("foo  "),
        "{only}"
    );
    let strict = f.ika(&["kb", "orphans", "-p", "foo", "--min", "4"]).out;
    assert!(
        strict.contains("foo  3 件\n") && !strict.contains("要整理"),
        "{strict}"
    );
}

#[test]
fn orphans_says_so_when_everything_is_organized() {
    let f = Fixture::new("orphans-none");
    f.write("kb/tech/notes/2026/2026-10-14-a.md", NOTE);
    f.write(
        "kb/tech/insights/i.md",
        &NOTE
            .replace("本文", "[a](../notes/2026/2026-10-14-a.md)")
            .replace("type: note", "type: insight"),
    );
    assert!(
        f.ika(&["kb", "orphans"])
            .out
            .contains("未整理のノートはありません")
    );
    assert!(
        f.ika(&["kb", "orphans", "-p", "nope"])
            .out
            .contains("未整理のノートはありません")
    );
}

// ---- ika kb triage ----

/// IKA_HOME 下の全ファイルの中身（読み取り専用の確認用）
fn snapshot(dir: &std::path::Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in fs::read_dir(&d).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let bytes = fs::read(&path).unwrap();
                out.push((path, bytes));
            }
        }
    }
    out.sort();
    out
}

/// foo のリポジトリ（README より後のコミットが1件）と、未整理のノート・知見のある KB
fn triage_fixture(name: &str) -> Fixture {
    let f = Fixture::new(name);
    let repo = f.dir.join("repo");
    fs::create_dir_all(&repo).unwrap();
    for args in [
        vec!["init", "-q"],
        vec!["commit", "-q", "--allow-empty", "-m", "after readme"],
    ] {
        let ok = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.test"])
            .args(&args)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?}");
    }
    f.write(
        "data/projects.csv",
        &format!(
            "name,path,url,status,summary\nfoo,{},,active,Foo\nbar,,,active,Bar\n",
            repo.display()
        ),
    );
    f.write(
        "kb/projects/bar/README.md",
        "---\ntitle: bar\ntype: project\ndate: 2026-10-14\nprojects: [bar]\ntags: []\n---\n\n# bar\n",
    );
    let note = |slug: &str, date: &str, projects: &str, tags: &str| {
        f.write(
            &format!("kb/tech/notes/2026/{date}-{slug}.md"),
            &format!(
                "---\ntitle: note {slug}\ntype: note\ndate: {date}\nprojects: {projects}\ntags: [{tags}]\n---\n\n本文\n"
            ),
        );
    };
    // foo: a・b・c・two の 4 件（auth を 3 件が共有）。bar: two・x の 2 件
    note("a", "2026-10-14", "[foo]", "auth");
    note("b", "2026-10-15", "[foo]", "auth");
    note("c", "2026-10-16", "[foo]", "auth, infra");
    note("two", "2026-10-17", "[foo, bar]", "infra");
    note("x", "2026-10-18", "[bar]", "");
    // 知見（2026-10-01）は、auth の新しいノートをまだ取り込んでいない
    f.write(
        "kb/tech/insights/auth.md",
        "---\ntitle: 認証の知見\ntype: insight\ndate: 2026-10-01\nprojects: [foo]\ntags: [auth]\n---\n\n本文\n",
    );
    f
}

#[test]
fn triage_ranks_distill_then_insight_then_readme_and_explains_each() {
    let f = triage_fixture("triage");
    let before = snapshot(&f.root());
    let run = f.ika(&["kb", "triage"]);
    assert!(run.ok, "{}", run.err);
    let out = run.out;

    // R2: 蒸留 → 知見 → README の順。番号つき
    let distill = out.find("1. 【蒸留】foo").expect(&out);
    let insight = out.find("2. 【知見】").expect(&out);
    let readme = out.find("3. 【README】foo").expect(&out);
    assert!(distill < insight && insight < readme, "{out}");
    assert!(out.contains("今週やること（3 件）"), "{out}");

    // R3: 4 件の foo は項目、2 件の bar は様子見
    assert!(out.contains("foo  未整理のノート 4 件"), "{out}");
    assert!(
        out.contains("様子見（未整理のノートが 3 件未満）: bar 2 件"),
        "{out}"
    );
    assert!(!out.contains("【蒸留】bar"), "{out}");

    // R4: 3 件が共有する auth と、2 件が共有する infra が候補。1 件だけのタグは出ない
    assert!(
        out.contains("まとめ方の候補: #auth（3 件）、#infra（2 件）"),
        "{out}"
    );
    // R5: 複数プロダクトのノートに、もう一方のプロダクト名が付く
    assert!(out.contains("note two（ほか bar にも関連）"), "{out}");
    assert!(!out.contains("note a（ほか"), "{out}");

    // R6: 直し方
    assert!(out.contains("distill.md"), "{out}");
    assert!(out.contains("project-sync"), "{out}");

    // 同じ入力なら同じ出力。KB には何も書かない（R10）
    assert_eq!(f.ika(&["kb", "triage"]).out, out);
    assert_eq!(snapshot(&f.root()), before);
}

#[test]
fn triage_min_moves_watched_projects_into_the_list() {
    let f = triage_fixture("triage-min");
    let out = f.ika(&["kb", "triage", "--min", "2"]).out;
    // bar（2 件）が番号に入り、foo（4 件）のあとに並ぶ
    let foo = out.find("1. 【蒸留】foo").expect(&out);
    let bar = out.find("2. 【蒸留】bar").expect(&out);
    assert!(foo < bar, "{out}");
    assert!(!out.contains("様子見"), "{out}");
    assert!(out.contains("note two（ほか foo にも関連）"), "{out}");
}

#[test]
fn triage_limit_hides_the_rest_and_points_to_orphans_and_stale() {
    let f = triage_fixture("triage-limit");
    let out = f.ika(&["kb", "triage", "--limit", "1"]).out;
    assert!(out.contains("今週やること（上位 1 件 / 全 3 件）"), "{out}");
    assert!(out.contains("1. 【蒸留】foo"), "{out}");
    assert!(!out.contains("2. "), "{out}");
    assert!(
        out.contains("ほか 2 件") && out.contains("ika kb orphans") && out.contains("ika kb stale"),
        "{out}"
    );
    let all = f.ika(&["kb", "triage", "--limit", "20"]).out;
    assert!(
        all.contains("3. 【README】foo") && !all.contains("ほか 2 件"),
        "{all}"
    );
}

#[test]
fn triage_project_filter_and_stale_options_are_passed_through() {
    let f = triage_fixture("triage-project");
    let out = f.ika(&["kb", "triage", "-p", "bar", "--min", "2"]).out;
    assert!(out.contains("【蒸留】bar"), "{out}");
    assert!(
        !out.contains("【蒸留】foo") && !out.contains("【README】"),
        "{out}"
    );

    // 古い価格のノートを根拠にする決定。--days と --volatile が stale と同じに効く
    f.write(
        "kb/tech/notes/2020/2020-01-01-cost.md",
        "---\ntitle: 古いコスト\ntype: note\ndate: 2020-01-01\nprojects: [foo]\ntags: [cost]\n---\n\n本文\n",
    );
    f.write(
        "kb/tech/decisions/2020-02-01-d.md",
        "---\ntitle: d\ntype: decision\ndate: 2020-02-01\nprojects: [foo]\ntags: []\nstatus: accepted\n---\n\n[根拠](../notes/2020/2020-01-01-cost.md)\n",
    );
    let default = f.ika(&["kb", "triage", "-p", "foo"]).out;
    assert!(
        default.contains("【根拠】") && default.contains("古いコスト"),
        "{default}"
    );
    let narrow = f
        .ika(&["kb", "triage", "-p", "foo", "--volatile", "pricing"])
        .out;
    assert!(!narrow.contains("【根拠】"), "{narrow}");
}

#[test]
fn triage_says_so_when_there_is_nothing_to_do() {
    let f = Fixture::new("triage-none");
    f.write("kb/tech/notes/2026/2026-10-14-a.md", NOTE);
    f.write(
        "kb/tech/insights/i.md",
        &NOTE
            .replace("本文", "[a](../notes/2026/2026-10-14-a.md)")
            .replace("type: note", "type: insight"),
    );
    let run = f.ika(&["kb", "triage"]);
    assert!(run.ok, "{}", run.err);
    assert_eq!(run.out.trim(), "今週やることはありません");
}

// ---- ika capture / ika inbox ----

fn today() -> String {
    // テストは日付の境目をまたがない前提で、`ika capture` が使うローカル日付と同じ形にする
    String::from_utf8(
        Command::new("date")
            .arg("+%Y-%m-%d")
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string()
}

#[test]
fn capture_appends_timestamped_entries_without_any_decisions() {
    let f = Fixture::new("capture");
    let day = today();

    // R1・R2: 引数（複数の語は空白でつなぐ）と、標準入力の複数行
    let first = f.ika(&["capture", "気になる", "メモ"]);
    assert!(first.ok, "{}", first.err);
    assert!(
        first.out.contains(&format!("inbox/{day}.md（今日 1 件）")),
        "{}",
        first.out
    );
    let second = f.ika_with(&["capture", "--stdin"], Some("一行目\n二行目\n"));
    assert!(second.ok, "{}", second.err);
    assert!(second.out.contains("今日 2 件"), "{}", second.out);

    let file = f.read(&format!("inbox/{day}.md"));
    assert_eq!(file.matches("\n## ").count() + 1, 2, "{file}");
    assert!(
        file.contains("気になる メモ") && file.contains("一行目\n二行目"),
        "{file}"
    );
    // 1回目の内容は消えない。フロントマターは付かない
    assert!(file.starts_with("## "), "{file}");

    // R3: URL もただの文字
    assert!(f.ika(&["capture", "https://example.test/x?a=1"]).ok);
    assert!(
        f.read(&format!("inbox/{day}.md"))
            .contains("https://example.test/x?a=1")
    );
}

#[test]
fn capture_refuses_empty_text_and_needs_ika_home() {
    let f = Fixture::new("capture-empty");
    assert!(!f.ika(&["capture", "   "]).ok);
    assert!(!f.ika_with(&["capture", "--stdin"], Some("\n \n")).ok);
    assert!(!f.exists("inbox"), "空の文章ではファイルを作らない");

    // R10
    let none = f.run(&["capture", "x"], None, None);
    assert!(!none.ok && none.err.contains("IKA_HOME"), "{}", none.err);
}

#[test]
fn capture_keeps_every_entry_when_run_concurrently() {
    let f = Fixture::new("capture-concurrent");
    let handles: Vec<_> = (0..6)
        .map(|n| {
            let root = f.root();
            let bin = env!("CARGO_BIN_EXE_ika");
            let cwd = f.dir.join("work");
            std::thread::spawn(move || {
                let out = Command::new(bin)
                    .current_dir(cwd)
                    .env("IKA_HOME", root)
                    .args(["capture", &format!("並行-{n}")])
                    .output()
                    .unwrap();
                assert!(out.status.success());
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    let file = f.read(&format!("inbox/{}.md", today()));
    for n in 0..6 {
        assert!(file.contains(&format!("並行-{n}")), "{file}");
    }
    assert_eq!(
        file.lines().filter(|l| l.starts_with("## ")).count(),
        6,
        "{file}"
    );
}

#[test]
fn capture_and_inbox_rm_each_make_one_commit() {
    let f = Fixture::bare("capture-commit");
    assert!(f.ika(&["init"]).ok);
    assert!(f.ika(&["capture", "メモ"]).ok);
    let log = f.git(&["log", "--format=%s"]);
    assert_eq!(log.lines().count(), 2, "{log}");
    assert!(log.lines().next().unwrap().contains("inbox"), "{log}");
    assert!(f.ika(&["inbox", "rm", &today()]).ok);
    let log = f.git(&["log", "--format=%s"]);
    assert_eq!(log.lines().count(), 3, "{log}");
    // 失敗した rm はコミットを増やさない
    assert!(!f.ika(&["inbox", "rm", &today()]).ok);
    assert_eq!(f.git(&["log", "--format=%s"]).lines().count(), 3);
}

#[test]
fn inbox_ls_lists_everything_in_order_and_rm_validates_the_date() {
    let f = Fixture::new("inbox-ls");
    assert!(f.ika(&["inbox", "ls"]).out.contains("受信箱は空です"));

    f.write(
        "inbox/2026-10-20.md",
        "## 09:05\n\n古いメモ\n\n## 14:32\n\n次\nの行\n\n",
    );
    f.write("inbox/2026-10-21.md", "## 08:00\n\n翌日\n\n");
    f.write("inbox/memo.txt", "日付ではない\n");
    let all = f.ika(&["inbox", "ls"]).out;
    let order: Vec<usize> = ["古いメモ", "次\nの行", "翌日"]
        .iter()
        .map(|t| all.find(t).expect(&all))
        .collect();
    assert!(order.windows(2).all(|w| w[0] < w[1]), "{all}");
    assert!(
        all.contains("■ 2026-10-20 09:05") && !all.contains("日付ではない"),
        "{all}"
    );
    let one = f.ika(&["inbox", "ls", "--date", "2026-10-21"]).out;
    assert!(one.contains("翌日") && !one.contains("古いメモ"), "{one}");

    // R7: 不正な日付・パス・存在しない日付は何も消さない
    for bad in ["2026-13-40", "../kb/x", "2026-1-5", "2026-10-22"] {
        assert!(!f.ika(&["inbox", "rm", bad]).ok, "{bad}");
    }
    assert!(f.exists("inbox/2026-10-20.md") && f.exists("inbox/2026-10-21.md"));
    let rm = f.ika(&["inbox", "rm", "2026-10-20"]);
    assert!(rm.ok && rm.out.contains("2 件"), "{}", rm.out);
    assert!(!f.exists("inbox/2026-10-20.md"));
    assert!(!f.ika(&["inbox", "ls"]).out.contains("古いメモ"));
}

#[test]
fn inbox_stays_out_of_the_kb_and_leads_the_triage() {
    let f = triage_fixture("inbox-triage");
    // 6 件: 先頭 5 件と「ほか 1 件」
    for n in 0..6 {
        f.write(
            &format!("inbox/2026-10-2{n}.md"),
            &format!("## 10:0{n}\n\n{}メモ{n}\n\n", "長い".repeat(40)),
        );
    }
    // R9: 受信箱は check・ls・検索に出ない
    let check = f.ika(&["check"]);
    assert!(check.ok, "{}", check.out);
    assert!(!f.ika(&["kb", "ls"]).out.contains("inbox"));
    assert!(!f.ika(&["kb", "search", "メモ", "-l"]).out.contains("inbox"));

    // R8
    let out = f.ika(&["kb", "triage"]).out;
    let inbox = out
        .find("1. 【受信箱】受信箱  6 件（6 日分、最も古いのは 2026-10-20）")
        .expect(&out);
    let distill = out.find("2. 【蒸留】foo").expect(&out);
    assert!(inbox < distill, "{out}");
    assert!(out.contains("ほか 1 件") && out.contains("…"), "{out}");
    assert!(
        out.contains("inbox.md") && out.contains("ika inbox rm"),
        "{out}"
    );
    assert!(
        out.contains("2026-10-20 10:00") && !out.contains("2026-10-25 10:05"),
        "{out}"
    );
    // プロダクトを絞ると出ない
    assert!(
        !f.ika(&["kb", "triage", "-p", "foo"])
            .out
            .contains("【受信箱】")
    );

    // 空になれば項目も消える
    for n in 0..6 {
        assert!(f.ika(&["inbox", "rm", &format!("2026-10-2{n}")]).ok);
    }
    assert!(!f.ika(&["kb", "triage"]).out.contains("【受信箱】"));
}

// ---- 書いた直後の提案（ika new / ika kb write）と ika check の注意 ----

fn note_with(title: &str, tags: &str, body: &str) -> String {
    format!(
        "---\ntitle: {title}\ntype: note\ndate: 2026-10-10\nprojects: [foo]\ntags: [{tags}]\n---\n\n{body}\n"
    )
}

#[test]
fn new_suggests_similar_notes_and_readme_links_on_stderr_only() {
    let f = Fixture::new("suggest");
    f.write(
        "kb/tech/notes/2026/2026-10-10-photo.md",
        &note_with("写真共有サービスの競合調査", "competitor", "本文"),
    );
    f.write(
        "kb/tech/notes/2026/2026-10-11-auth.md",
        &note_with("認証の実装メモ", "auth", "本文"),
    );
    // type: source は照合しない
    f.write(
        "kb/tech/notes/2026/2026-10-12-src.md",
        &note_with("写真共有サービスの競合調査", "competitor", "本文")
            .replace("type: note", "type: source\nsource: x"),
    );

    let run = f.ika_with(
        &[
            "new",
            "note",
            "写真共有アプリの競合比較",
            "-d",
            "tech",
            "-p",
            "foo",
            "-t",
            "competitor",
            "--slug",
            "probe",
            "--stdin",
        ],
        Some("本文\n"),
    );
    assert!(run.ok, "{}", run.err);
    // R1・R7: 標準出力は作ったパスだけ。提案は標準エラー
    let day = today();
    assert_eq!(run.out.trim(), format!("kb/tech/notes/2026/{day}-probe.md"));
    let err = run.err;
    assert!(err.contains("似た既存のナレッジ"), "{err}");
    assert!(
        err.contains("2026-10-10-photo.md") && err.contains("[note] 2026-10-10"),
        "{err}"
    );
    assert!(err.contains("共通の語:") && err.contains("写真"), "{err}");
    assert!(err.contains("追記を"), "{err}");
    // 無関係なもの・ソース・自分自身は出ない
    assert!(
        !err.contains("2026-10-11-auth.md") && !err.contains("2026-10-12-src.md"),
        "{err}"
    );
    assert!(!err.contains(&format!("{day}-probe.md")), "{err}");
    // R5: README にリンクが無いので候補に出る
    assert!(
        err.contains("README の「関連」に足す候補: kb/projects/foo/README.md"),
        "{err}"
    );
}

#[test]
fn new_skips_readme_suggestion_when_already_linked_and_ignores_unrelated_titles() {
    let f = Fixture::new("suggest-readme");
    let day = today();
    f.write(
        "kb/projects/foo/README.md",
        &format!(
            "---\ntitle: foo\ntype: project\ndate: 2026-10-14\nprojects: [foo]\ntags: []\n---\n\n[x](../../tech/notes/{}/{day}-probe.md)\n",
            &day[..4]
        ),
    );
    f.write(
        "kb/tech/notes/2026/2026-10-11-auth.md",
        &note_with("認証の実装メモ", "auth", "本文"),
    );
    let run = f.ika_with(
        &[
            "new",
            "note",
            "支払いの返金処理",
            "-d",
            "tech",
            "-p",
            "foo",
            "--slug",
            "probe",
            "--stdin",
        ],
        Some("本文\n"),
    );
    assert!(run.ok, "{}", run.err);
    // 似たものも README の候補も無ければ、何も出ない
    assert!(run.err.trim().is_empty(), "{}", run.err);
}

#[test]
fn new_ignores_generic_words_in_big_kbs_but_keeps_specific_ones() {
    let f = Fixture::new("suggest-generic");
    for n in 0..30 {
        f.write(
            &format!("kb/tech/notes/2026/2026-10-{:02}-filler{n}.md", 1 + n % 9),
            &note_with(
                &format!("調査メモ {n} まとめ"),
                "misc",
                "調査の結果をまとめた",
            ),
        );
    }
    f.write(
        "kb/tech/notes/2026/2026-10-10-token.md",
        &note_with("認証トークンの更新調査", "auth", "本文"),
    );
    let new = |title: &str, slug: &str| {
        f.ika_with(
            &[
                "new", "note", title, "-d", "tech", "--slug", slug, "--stdin",
            ],
            Some("本文\n"),
        )
    };
    // 「調査」「まとめ」だけでは似ているとは言えない
    let generic = new("調査まとめ", "generic");
    assert!(
        generic.ok && !generic.err.contains("似た既存"),
        "{}",
        generic.err
    );
    // 一般的な語（調査）を除いても、認証・トークンが共通なら出る
    let specific = new("認証トークンの失効調査", "specific");
    assert!(
        specific.err.contains("2026-10-10-token.md") && !specific.err.contains("filler"),
        "{}",
        specific.err
    );
}

#[test]
fn kb_write_suggests_for_new_files_only_and_failures_never_break_the_write() {
    let f = Fixture::new("suggest-write");
    f.write(
        "kb/tech/notes/2026/2026-10-10-photo.md",
        &note_with("写真共有サービスの競合調査", "competitor", "本文"),
    );
    let text = note_with("写真共有アプリの競合比較", "competitor", "本文");
    let run = f.ika_with(
        &["kb", "write", "kb/tech/notes/2026/2026-10-20-w.md"],
        Some(&text),
    );
    assert!(run.ok, "{}", run.err);
    assert!(run.err.contains("2026-10-10-photo.md"), "{}", run.err);
    assert!(!run.err.contains("2026-10-20-w.md"), "{}", run.err);

    // R2: --overwrite では出さない
    let again = f.ika_with(
        &[
            "kb",
            "write",
            "kb/tech/notes/2026/2026-10-20-w.md",
            "--overwrite",
        ],
        Some(&text),
    );
    assert!(again.ok && !again.err.contains("似た既存"), "{}", again.err);

    // R6: 索引を作れなくても書き込みは成功し、標準出力は変わらない
    fs::remove_dir_all(f.root().join(".cache")).unwrap();
    fs::write(
        f.root().join(".cache"),
        "ファイルがディレクトリの場所にある",
    )
    .unwrap();
    let broken = f.ika_with(
        &["kb", "write", "kb/tech/notes/2026/2026-10-21-w.md"],
        Some(&text),
    );
    assert!(broken.ok, "{}", broken.err);
    assert!(
        broken.out.contains("書きました") && !broken.err.contains("似た既存"),
        "{}",
        broken.err
    );
    assert!(f.exists("kb/tech/notes/2026/2026-10-21-w.md"));
}

#[test]
fn check_notes_recent_unlinked_notes_without_failing() {
    let f = Fixture::new("check-notice");
    let day = today();
    let recent = |slug: &str| format!("kb/tech/notes/2026/{day}-{slug}.md");
    let body = |title: &str, date: &str| {
        NOTE.replace("title: n", &format!("title: {title}"))
            .replace("date: 2026-10-14", &format!("date: {date}"))
    };
    f.write(&recent("lonely"), &body("孤立", &day));
    f.write(&recent("linked"), &body("リンクあり", &day));
    f.write(
        "kb/tech/notes/2020/2020-01-01-old.md",
        &body("古い孤立", "2020-01-01"),
    );
    f.write(
        "kb/tech/insights/i.md",
        &NOTE
            .replace("本文", &format!("[x](../notes/2026/{day}-linked.md)"))
            .replace("type: note", "type: insight"),
    );
    let run = f.ika(&["check"]);
    // 注意が出ても成功し、OK も出る
    assert!(run.ok && run.out.contains("OK（"), "{}", run.out);
    assert!(
        run.out.contains("注意（失敗にはしません）")
            && run.out.contains(&format!("{day}-lonely.md")),
        "{}",
        run.out
    );
    assert!(
        !run.out.contains("-linked.md") && !run.out.contains("2020-01-01-old.md"),
        "{}",
        run.out
    );
}

// ---- ika install / ika check（プラグインの version）----

#[cfg(unix)]
mod plugin_version {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// git 管理された最小の ika リポジトリ（プラグインのマニフェスト付き）と、偽の `claude`
    struct Plugin<'a> {
        f: &'a Fixture,
        repo: PathBuf,
        bin: PathBuf,
        log: PathBuf,
        installed: PathBuf,
    }

    impl<'a> Plugin<'a> {
        fn new(f: &'a Fixture) -> Self {
            let repo = f.repo();
            let manifest =
                "{\n  \"name\": \"ika\",\n  \"version\": \"0.1.0\",\n  \"description\": \"d\"\n}\n";
            fs::create_dir_all(repo.join("plugins/ika/.claude-plugin")).unwrap();
            fs::write(
                repo.join("plugins/ika/.claude-plugin/plugin.json"),
                manifest,
            )
            .unwrap();
            fs::write(repo.join("plugins/ika/plugin.json"), manifest).unwrap();
            let git = |args: &[&str]| {
                let out = Command::new("git")
                    .arg("-C")
                    .arg(&repo)
                    .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
                    .args(args)
                    .output()
                    .unwrap();
                assert!(
                    out.status.success(),
                    "{args:?}: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
            };
            git(&["init", "-q"]);
            git(&["add", "-A"]);
            git(&["commit", "-q", "-m", "init"]);

            // 偽の claude: 呼ばれた引数を記録し、install / update ならプラグインをコピーする
            let bin = f.dir.join("fakebin");
            fs::create_dir_all(&bin).unwrap();
            let script = bin.join("claude");
            fs::write(
                &script,
                "#!/bin/sh\necho \"$@\" >> \"$FAKE_LOG\"\n\
                 case \"$*\" in\n  \"plugin update\"*|\"plugin install\"*)\n\
                 if [ -n \"$FAKE_COPY\" ]; then /bin/rm -rf \"$FAKE_INSTALLED\"; /bin/mkdir -p \"$FAKE_INSTALLED\"; \
                 /bin/cp -R \"$FAKE_REPO/plugins/ika/.\" \"$FAKE_INSTALLED/\"; fi;;\nesac\n",
            )
            .unwrap();
            fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
            Plugin {
                f,
                repo,
                bin,
                log: f.dir.join("claude.log"),
                installed: f.dir.join("installed-ika"),
            }
        }

        /// インストール済みの状態にする（いまのリポジトリの内容をコピー）
        fn mark_installed(&self) {
            fs::create_dir_all(&self.installed).unwrap();
            let out = Command::new("cp")
                .arg("-R")
                .arg(self.repo.join("plugins/ika/."))
                .arg(&self.installed)
                .output()
                .unwrap();
            assert!(out.status.success());
            let dir = self.f.dir.join(".claude/plugins");
            fs::create_dir_all(&dir).unwrap();
            fs::write(
                dir.join("installed_plugins.json"),
                format!(
                    "{{\"plugins\":{{\"ika@ika\":[{{\"installPath\":\"{}\",\"version\":\"0.1.0\"}}]}}}}",
                    self.installed.display()
                ),
            )
            .unwrap();
        }

        fn write(&self, rel: &str, text: &str) {
            let path = self.repo.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }

        fn version(&self) -> (String, String) {
            let read = |rel: &str| fs::read_to_string(self.repo.join(rel)).unwrap();
            (
                read("plugins/ika/.claude-plugin/plugin.json"),
                read("plugins/ika/plugin.json"),
            )
        }

        fn check(&self) -> Run {
            self.f
                .run_in(&self.repo, &["check"], None, Some(self.f.root()))
        }

        /// 偽の claude だけが PATH にある状態で `ika install ...`。`copy` なら成功したように見せかける
        fn install(&self, args: &[&str], copy: bool) -> Run {
            let mut command = Command::new(env!("CARGO_BIN_EXE_ika"));
            command
                .arg("install")
                .args(args)
                .env("IKA_HOME", self.f.root())
                .env("HOME", &self.f.dir)
                .env("PATH", &self.bin)
                .env("FAKE_LOG", &self.log)
                .env("FAKE_REPO", &self.repo)
                .env("FAKE_INSTALLED", &self.installed)
                .env_remove("CLAUDE_CONFIG_DIR")
                .current_dir(&self.repo);
            if copy {
                command.env("FAKE_COPY", "1");
            }
            let out = command.output().unwrap();
            Run {
                ok: out.status.success(),
                out: String::from_utf8(out.stdout).unwrap(),
                err: String::from_utf8(out.stderr).unwrap(),
            }
        }

        fn claude_calls(&self) -> String {
            fs::read_to_string(&self.log).unwrap_or_default()
        }
    }

    const NOT_BUMPED: &str = "のときから";

    #[test]
    fn check_flags_plugin_changes_made_without_bumping_the_version() {
        let f = Fixture::new("plugin-check");
        let p = Plugin::new(&f);
        assert!(!p.check().out.contains(NOT_BUMPED), "{}", p.check().out);

        p.write(
            "plugins/ika/skills/demo/SKILL.md",
            "---\nname: demo\ndescription: 変えた\n---\n",
        );
        let changed = p.check();
        assert!(!changed.ok);
        assert!(
            changed
                .out
                .contains("version 0.1.0 のときから 1 ファイルが変わっています"),
            "{}",
            changed.out
        );
        assert!(
            changed.out.contains("skills/demo/SKILL.md")
                && changed.out.contains("ika install --bump patch")
        );

        // 新規ファイル（未追跡）も数える
        p.write("plugins/ika/skills/demo/references/new.md", "x\n");
        assert!(p.check().out.contains("2 ファイルが変わっています"));

        // マニフェストの version を上げれば消える
        let (claude, openai) = p.version();
        p.write(
            "plugins/ika/.claude-plugin/plugin.json",
            &claude.replace("0.1.0", "0.1.1"),
        );
        p.write("plugins/ika/plugin.json", &openai.replace("0.1.0", "0.1.1"));
        assert!(!p.check().out.contains(NOT_BUMPED), "{}", p.check().out);
    }

    #[test]
    fn check_is_quiet_when_nothing_in_the_plugin_changed() {
        let f = Fixture::new("plugin-check-quiet");
        let p = Plugin::new(&f);
        // プラグイン外の変更は関係ない
        p.write("docs/other.md", "x\n");
        assert!(!p.check().out.contains(NOT_BUMPED));
    }

    #[test]
    fn check_notes_a_stale_installed_copy_without_failing_on_it() {
        let f = Fixture::new("plugin-check-installed");
        let p = Plugin::new(&f);
        p.mark_installed();
        assert!(!p.check().out.contains("古いコピー"), "{}", p.check().out);

        fs::write(p.installed.join("skills/demo/SKILL.md"), "古い内容\n").unwrap();
        let run = p.check();
        assert!(
            run.out.contains(
                "インストール済みの ika プラグインは古いコピーです: 1 ファイルが違います"
            ),
            "{}",
            run.out
        );
    }

    #[test]
    fn install_bumps_the_version_in_both_manifests_and_updates_the_installed_plugin() {
        let f = Fixture::new("plugin-install-update");
        let p = Plugin::new(&f);
        p.mark_installed();
        p.write(
            "plugins/ika/skills/demo/SKILL.md",
            "---\nname: demo\ndescription: 新しい\n---\n",
        );

        let run = p.install(&["--bump", "patch"], true);
        assert!(run.ok, "{}{}", run.out, run.err);
        assert!(run.out.contains("version: 0.1.0 -> 0.1.1"), "{}", run.out);
        let (claude, openai) = p.version();
        assert!(
            claude.contains("\"version\": \"0.1.1\"") && openai.contains("\"version\": \"0.1.1\"")
        );
        assert!(
            claude.contains("\"description\": \"d\""),
            "ほかの書式は崩さない"
        );

        let calls = p.claude_calls();
        assert!(calls.contains("plugin marketplace add"), "{calls}");
        assert!(calls.contains("plugin marketplace update ika"), "{calls}");
        assert!(
            calls.contains("plugin update ika@ika") && !calls.contains("plugin install"),
            "{calls}"
        );
        assert!(
            run.out
                .contains("リポジトリと同じ内容です（version 0.1.1）"),
            "{}",
            run.out
        );
        assert!(run.out.contains("再起動"));
        // 入れ直したあとは、check の指摘も出ない
        assert!(!p.check().out.contains(NOT_BUMPED));
    }

    #[test]
    fn install_installs_when_the_plugin_is_not_installed_yet() {
        let f = Fixture::new("plugin-install-new");
        let p = Plugin::new(&f);
        let run = p.install(&[], true);
        let calls = p.claude_calls();
        assert!(
            calls.contains("plugin install ika@ika") && !calls.contains("plugin update"),
            "{calls}"
        );
        // 偽の claude は installed_plugins.json を作らないので、入らなかったものとして失敗にする
        assert!(!run.ok);
        assert!(
            run.out.contains("まだインストールされていません"),
            "{}",
            run.out
        );
    }

    #[test]
    fn install_fails_loudly_when_the_copy_stays_stale() {
        let f = Fixture::new("plugin-install-stale");
        let p = Plugin::new(&f);
        p.mark_installed();
        p.write(
            "plugins/ika/skills/demo/SKILL.md",
            "---\nname: demo\ndescription: 新しい\n---\n",
        );

        // version を上げず、claude も更新しなかった（version が同じだと更新されない状況）
        let run = p.install(&[], false);
        assert!(!run.ok);
        assert!(
            run.out
                .contains("まだ古いコピーです（1 ファイルが違います: skills/demo/SKILL.md）"),
            "{}",
            run.out
        );
        assert!(run.out.contains("ika install --bump patch"));
        assert!(run.err.contains("更新できませんでした"), "{}", run.err);
    }

    #[test]
    fn install_dry_run_changes_and_runs_nothing() {
        let f = Fixture::new("plugin-install-dry");
        let p = Plugin::new(&f);
        p.mark_installed();
        let before = p.version();
        let run = p.install(&["--dry-run", "--bump", "minor"], true);
        assert!(run.ok, "{}{}", run.out, run.err);
        assert!(
            run.out
                .contains("version: 0.1.0 -> 0.2.0（dry-run のため書き換えません）"),
            "{}",
            run.out
        );
        assert!(run.out.contains("$ claude plugin update ika@ika"));
        assert_eq!(p.version(), before);
        assert_eq!(p.claude_calls(), "");
    }

    #[test]
    fn install_rejects_a_malformed_version() {
        let f = Fixture::new("plugin-install-badver");
        let p = Plugin::new(&f);
        let (claude, openai) = p.version();
        p.write(
            "plugins/ika/.claude-plugin/plugin.json",
            &claude.replace("0.1.0", "1.2"),
        );
        p.write("plugins/ika/plugin.json", &openai.replace("0.1.0", "1.2"));
        let run = p.install(&["--bump", "patch"], false);
        assert!(!run.ok);
        assert!(run.err.contains("x.y.z"), "{}", run.err);
    }
}

// ---- ika data（SQLite のデータセット）----

mod data {
    use super::*;

    const PRICES: [&str; 11] = [
        "data",
        "new",
        "prices",
        "--summary",
        "価格の履歴",
        "--col",
        "date:date",
        "--col",
        "kind:enum(sold|listed)",
        "--col",
        "price:int",
    ];

    /// `prices` を作る（`--key date,kind`、`url` は省略可）
    fn prices(f: &Fixture) {
        let mut args = PRICES.to_vec();
        args.extend(["--col", "url:string?", "--key", "date,kind"]);
        let run = f.ika(&args);
        assert!(run.ok, "{}", run.err);
    }

    #[test]
    fn append_validates_the_whole_input_before_writing() {
        let f = Fixture::new("data-validate");
        prices(&f);
        let bad = f.ika_with(
            &["data", "append", "prices"],
            Some("date,kind,price\n2026-10-18,sold,100\n先週,bought,1.5\n"),
        );
        assert!(!bad.ok);
        assert!(bad.err.contains("3 行目: date"), "{}", bad.err);
        assert!(bad.err.contains("3 行目: kind"), "{}", bad.err);
        assert!(bad.err.contains("3 行目: price"), "{}", bad.err);
        let listed = f.ika(&["data", "ls"]).out;
        assert!(
            listed.contains("0 行"),
            "1行目が正しくても、不正な行があれば何も入らない: {listed}"
        );

        let ok = f.ika_with(
            &["data", "append", "prices"],
            Some("price,date,kind,url\n100,2026-10-18,sold,https://example.com\n"),
        );
        assert!(ok.ok, "{}", ok.err);
        let rows = f.ika(&["data", "sql", "select date, kind, price, url from prices"]);
        assert_eq!(
            rows.out, "date\tkind\tprice\turl\n2026-10-18\tsold\t100\thttps://example.com\n",
            "列の順番は自由"
        );
    }

    #[test]
    fn append_rejects_unknown_and_missing_columns() {
        let f = Fixture::new("data-columns");
        prices(&f);
        let unknown = f.ika_with(
            &["data", "append", "prices"],
            Some("date,kind,price,memo\n2026-10-18,sold,1,x\n"),
        );
        assert!(!unknown.ok && unknown.err.contains("定義にない列"));
        let missing = f.ika_with(
            &["data", "append", "prices"],
            Some("date,kind\n2026-10-18,sold\n"),
        );
        assert!(!missing.ok && missing.err.contains("必須の列がありません: price"));
    }

    #[test]
    fn key_conflicts_error_skip_or_replace() {
        let f = Fixture::new("data-conflict");
        prices(&f);
        let row = |price: u32| format!("date,kind,price\n2026-10-18,sold,{price}\n");
        assert!(
            f.ika_with(&["data", "append", "prices"], Some(&row(100)))
                .ok
        );

        let dup = f.ika_with(&["data", "append", "prices"], Some(&row(200)));
        assert!(!dup.ok && dup.err.contains("重複"), "{}", dup.err);

        let skip = f.ika_with(
            &["data", "append", "prices", "--on-conflict", "skip"],
            Some(&row(200)),
        );
        assert!(skip.ok && skip.out.contains("重複 1 行"), "{}", skip.out);
        let price = |f: &Fixture| f.ika(&["data", "sql", "select price from prices"]).out;
        assert_eq!(price(&f), "price\n100\n");

        let replace = f.ika_with(
            &["data", "append", "prices", "--on-conflict", "replace"],
            Some(&row(300)),
        );
        assert!(replace.ok, "{}", replace.err);
        assert_eq!(price(&f), "price\n300\n");
    }

    #[test]
    fn sql_is_read_only_and_can_emit_json() {
        let f = Fixture::new("data-sql");
        prices(&f);
        assert!(
            f.ika_with(
                &["data", "append", "prices"],
                Some("date,kind,price\n2026-10-18,sold,100\n")
            )
            .ok
        );
        for query in [
            "delete from prices",
            "drop table prices",
            "insert into prices values ('2026-10-19','sold',1,null)",
        ] {
            let run = f.ika(&["data", "sql", query]);
            assert!(!run.ok, "{query}");
        }
        assert_eq!(
            f.ika(&["data", "sql", "select count(*) n from prices"]).out,
            "n\n1\n"
        );

        let json = f.ika(&["data", "sql", "select price, url from prices", "--json"]);
        let value: serde_json::Value = serde_json::from_str(&json.out).unwrap();
        assert_eq!(value[0]["price"], 100);
        assert!(value[0]["url"].is_null());
    }

    #[test]
    fn export_delete_and_rm() {
        let f = Fixture::new("data-manage");
        prices(&f);
        let comma = f.ika_with(
            &["data", "append", "prices"],
            Some("date,kind,price\n2026-10-18,sold,100\n2026-10-19,listed,\"1,200\"\n"),
        );
        assert!(!comma.ok, "カンマ入りの数値は整数ではない");
        assert!(
            f.ika_with(
                &["data", "append", "prices"],
                Some("date,kind,price\n2026-10-18,sold,100\n2026-10-19,listed,200\n")
            )
            .ok
        );
        assert_eq!(
            f.ika(&["data", "export", "prices"]).out,
            "date,kind,price,url\n2026-10-18,sold,100,\n2026-10-19,listed,200,\n"
        );

        let del = f.ika(&["data", "delete", "prices", "--where", "kind = 'listed'"]);
        assert!(del.ok && del.out.contains("1 行"), "{}", del.err);
        assert!(
            !f.ika(&[
                "data",
                "delete",
                "prices",
                "--where",
                "1; drop table prices"
            ])
            .ok
        );
        assert!(f.ika(&["data", "show", "prices"]).out.contains("行数: 1"));

        assert!(f.ika(&["data", "rm", "prices"]).ok);
        assert!(!f.ika(&["data", "show", "prices"]).ok);
        assert!(f.ika(&["check"]).ok);
    }

    #[test]
    fn new_validates_definitions() {
        let f = Fixture::new("data-new");
        let run = |args: &[&str]| f.ika(&[&["data", "new"], args].concat());
        let base = ["x", "--summary", "s"];
        for (bad, why) in [
            (vec!["Bad-Name", "--summary", "s", "--col", "a:int"], "名前"),
            (vec![base[0], base[1], base[2], "--col", "a:money"], "型"),
            (
                vec![
                    base[0], base[1], base[2], "--col", "a:int", "--col", "a:int",
                ],
                "列の重複",
            ),
            (
                vec![base[0], base[1], base[2], "--col", "a:int", "--key", "b"],
                "key が未定義の列",
            ),
            (
                vec![base[0], base[1], base[2], "--col", "a:int?", "--key", "a"],
                "key が省略可の列",
            ),
            (
                vec![
                    base[0], base[1], base[2], "--col", "a:int", "--domain", "nope",
                ],
                "未登録の領域",
            ),
            (
                vec![
                    base[0],
                    base[1],
                    base[2],
                    "--col",
                    "a:int",
                    "--project",
                    "nope",
                ],
                "未登録のプロダクト",
            ),
        ] {
            assert!(!run(&bad).ok, "{why}");
        }
        assert!(
            run(&[
                base[0],
                base[1],
                base[2],
                "--col",
                "a:int",
                "--domain",
                "tech",
                "--project",
                "foo"
            ])
            .ok
        );
        assert!(
            !run(&[base[0], base[1], base[2], "--col", "a:int"]).ok,
            "同名は作れない"
        );
    }

    #[test]
    fn writes_are_committed_and_check_catches_drift() {
        let f = Fixture::new("data-git");
        let git_init = Command::new("git")
            .args(["init", "-q"])
            .arg(f.root())
            .status()
            .unwrap();
        assert!(git_init.success());
        prices(&f);
        assert!(
            f.ika_with(
                &["data", "append", "prices"],
                Some("date,kind,price\n2026-10-18,sold,100\n")
            )
            .ok
        );
        let log = f.git(&["log", "--format=%s"]);
        assert!(
            log.contains("data new prices") && log.contains("data append prices (1 rows)"),
            "{log}"
        );
        assert!(f.exists("data/datasets.sqlite3"));
        assert!(f.ika(&["check"]).ok);

        // 手で作ったテーブルは検出される
        let conn = rusqlite::Connection::open(f.root().join("data/datasets.sqlite3")).unwrap();
        conn.execute("CREATE TABLE stray (a)", []).unwrap();
        drop(conn);
        let check = f.ika(&["check"]);
        assert!(!check.ok && check.out.contains("stray"), "{}", check.out);
    }
}

#[test]
fn new_spec_goes_under_the_project_and_passes_check() {
    let f = Fixture::new("spec");
    let base = ["new", "spec", "Shift export", "--date", "2026-10-21"];

    let with_domain = [&base[..], &["--project", "foo", "--domain", "tech"]].concat();
    assert!(
        !f.ika_with(&with_domain, Some("")).ok,
        "spec に領域は付けない"
    );
    assert!(!f.ika_with(&base, Some("")).ok, "プロダクトが必要");
    let two = [&base[..], &["--project", "foo", "--project", "foo"]].concat();
    assert!(!f.ika_with(&two, Some("")).ok, "プロダクトは1つだけ");

    let run = f.ika_with(&[&base[..], &["--project", "foo"]].concat(), None);
    assert!(run.ok, "{}", run.err);
    let path = "kb/projects/foo/specs/shift-export.md";
    assert_eq!(run.out.trim(), path);
    let text = f.read(path);
    assert!(text.contains("type: spec\n") && text.contains("status: draft\n"));
    assert!(text.contains("受け入れ条件"));

    let check = f.ika(&["check"]);
    assert!(check.ok, "{}{}", check.out, check.err);

    let show = f.ika(&["project", "show", "foo"]);
    assert!(show.out.contains("[spec draft]"), "{}", show.out);

    f.ika(&[
        "kb",
        "edit",
        path,
        "--old",
        "status: draft",
        "--new",
        "status: wip",
    ]);
    let check = f.ika(&["check"]);
    assert!(!check.ok);
    assert!(format!("{}{}", check.out, check.err).contains("draft / approved / done / dropped"));
}

#[test]
fn import_writes_a_source_and_reimports_only_when_changed() {
    let f = Fixture::new("import-source");
    let original = f.dir.join("work/spec.md");
    fs::write(
        &original,
        "---\nauthor: x\n---\n# Shift Spec\n\n本文 [設計](design.md)\n",
    )
    .unwrap();
    let path = original.to_str().unwrap();

    assert!(!f.ika(&["import", path]).ok, "置き場所が必要");
    let run = f.ika(&["import", path, "--project", "foo", "--tags", "spec"]);
    assert!(run.ok, "{}", run.err);
    let rel = "kb/projects/foo/sources/spec.md";
    assert!(run.out.contains(rel), "{}", run.out);
    let text = f.read(rel);
    assert!(text.contains("title: Shift Spec\ntype: source\n"), "{text}");
    assert!(text.contains("tags: [spec]\nsource: /"), "{text}");
    assert!(text.contains("format: md\nhash: "));
    assert!(!text.contains("author: x"), "原本のフロントマターは外す");
    assert!(text.contains("# Shift Spec\n\n本文 [設計](design.md)"));

    let check = f.ika(&["check"]);
    assert!(
        check.ok,
        "原本の相対リンクは確かめない: {}{}",
        check.out, check.err
    );

    let again = f.ika(&["import", path, "--project", "foo"]);
    assert!(again.out.contains("変更なし"), "{}", again.out);

    fs::write(&original, "# Shift Spec\n\n本文を直した\n追記\n").unwrap();
    let changed = f.ika(&["import", path, "--project", "foo"]);
    assert!(
        changed.out.contains("取り込み直しました（+2 -1 行）"),
        "{}",
        changed.out
    );
    let text = f.read(rel);
    assert!(
        text.contains("本文を直した") && text.contains("tags: [spec]"),
        "{text}"
    );

    let list = f.ika(&["import", "--list", "-p", "foo"]);
    assert!(
        list.out.contains(rel) && list.out.contains(path),
        "{}",
        list.out
    );
}

#[test]
fn import_from_stdin_needs_source_and_title() {
    let f = Fixture::new("import-stdin");
    let base = ["import", "-", "--domain", "tech"];
    assert!(!f.ika_with(&base, Some("本文\n")).ok, "--source が必要");
    let with_source = [&base[..], &["--source", "https://notion.so/page-1"]].concat();
    assert!(
        !f.ika_with(&with_source, Some("本文\n")).ok,
        "--title が必要"
    );

    let full = [
        &with_source[..],
        &["--title", "料金表", "--slug", "pricing"],
    ]
    .concat();
    let run = f.ika_with(&full, Some("| a | b |\n"));
    assert!(run.ok, "{}", run.err);
    let text = f.read("kb/tech/sources/pricing.md");
    assert!(text.contains("source: https://notion.so/page-1"));
    assert!(text.contains("projects: []"));
    assert!(f.ika(&["check"]).ok);

    let print = f.ika_with(
        &["import", "-", "--print", "--format", "csv"],
        Some("x,y\n1,2\n"),
    );
    assert_eq!(print.out, "| x | y |\n|---|---|\n| 1 | 2 |\n");
}

#[test]
fn import_falls_back_to_the_directory_when_file_names_collide() {
    let f = Fixture::new("import-collide");
    for dir in ["20260617-photo-comment", "20260930-web-push"] {
        fs::create_dir_all(f.dir.join("work/docs").join(dir)).unwrap();
        fs::write(
            f.dir.join("work/docs").join(dir).join("requirements.md"),
            format!("# 要件定義 {dir}\n"),
        )
        .unwrap();
    }
    let a = f
        .dir
        .join("work/docs/20260617-photo-comment/requirements.md");
    let b = f.dir.join("work/docs/20260930-web-push/requirements.md");
    let run = f.ika(&[
        "import",
        a.to_str().unwrap(),
        b.to_str().unwrap(),
        "--project",
        "foo",
    ]);
    assert!(run.ok, "{}", run.err);
    assert!(f.exists("kb/projects/foo/sources/requirements.md"));
    assert!(f.exists("kb/projects/foo/sources/20260930-web-push-requirements.md"));

    let again = f.ika(&["import", b.to_str().unwrap(), "--project", "foo"]);
    assert!(
        again.out.contains("変更なし"),
        "原本で見分ける: {}",
        again.out
    );
}

#[test]
fn edit_accepts_text_starting_with_a_hyphen() {
    let f = Fixture::new("edit-hyphen");
    let path = "kb/tech/notes/2026/2026-10-14-list.md";
    f.write(
        path,
        "---\ntitle: t\ntype: note\ndate: 2026-10-14\nprojects: []\ntags: []\n---\n\n- 古い項目\n",
    );
    let run = f.ika(&[
        "kb",
        "edit",
        path,
        "--old",
        "- 古い項目",
        "--new",
        "- 新しい項目",
    ]);
    assert!(run.ok, "{}", run.err);
    assert!(f.read(path).contains("- 新しい項目"));
}

// ---- ika export html ----

/// 1×1 の PNG
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d, 0x49, 0x48, 0x44, 0x52, 0, 0, 0,
    1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0x1f, 0x15, 0xc4, 0x89,
];

fn doc(kind: &str, title: &str, date: &str, status: &str, body: &str) -> String {
    let status = if status.is_empty() {
        String::new()
    } else {
        format!("status: {status}\n")
    };
    format!(
        "---\ntitle: {title}\ntype: {kind}\ndate: {date}\nprojects: [foo]\ntags: [t]\n{status}---\n\n{body}\n"
    )
}

#[test]
fn export_html_writes_a_standalone_page_without_scripts_or_external_loads() {
    let f = Fixture::new("export-html-page");
    fs::write(f.root().join("kb/tech/notes/2026/p.png"), PNG).unwrap();
    let note = doc(
        "note",
        "図のノート",
        "2026-10-23",
        "",
        "## 流れ\n\n```seq 呼び出し\n利用者 => ika: 打つ\nika --> 利用者: 開く\n```\n\n\
         > [!NOTE]\n> メモ\n\n<script>alert(1)</script>\n\n![図](p.png) ![外](https://example.test/x.png)\n\n\
         [README](../../../projects/foo/README.md) [外部](https://example.test/)\n",
    );
    f.write("kb/tech/notes/2026/2026-10-23-a.md", &note);

    let run = f.ika(&["export", "html", "kb/tech/notes/2026/2026-10-23-a.md"]);
    assert!(run.ok, "{}", run.err);
    let out = ".cache/html/kb/tech/notes/2026/2026-10-23-a.html";
    assert!(run.out.contains(out), "{}", run.out);
    let html = f.read(out);
    // R1: スクリプトなし・外部の読み込みなし・CSS はインライン
    assert!(!html.contains("<script"), "{html}");
    assert!(html.contains("&lt;script&gt;"));
    assert!(
        html.contains("content=\"default-src 'none'; style-src 'unsafe-inline'; img-src data:\"")
    );
    assert!(
        !html.contains("src=\"http") && !html.contains("<link"),
        "{html}"
    );
    assert!(html.contains("<style>") && html.contains(".ikd-seq-grid"));
    // R5: 図と、説明つきの図の番号
    assert!(html.contains("<div class=\"ikd ikd-seq\">"));
    assert!(html.contains("<figcaption><b>図1</b>呼び出し</figcaption>"));
    assert!(html.contains("<blockquote class=\"markdown-alert-note\">"));
    // R4: 画像は data URI。外部の画像はリンクにする
    assert!(
        html.contains("src=\"data:image/png;base64,iVBORw0KGgo"),
        "{html}"
    );
    assert!(html.contains("<a href=\"https://example.test/x.png\">画像: 外</a>"));
    // R3: 書き出していないページはリンクを外し、パスを添える。外部のリンクはそのまま
    assert!(html.contains(
        "<span class=\"nolink\">README</span><span class=\"kbpath\">kb/projects/foo/README.md</span>"
    ));
    assert!(html.contains("<a href=\"https://example.test/\">外部</a>"));
    // 目次とメタ情報
    assert!(html.contains("<a href=\"#流れ\">流れ</a>"));
    assert!(html.contains("<dt>種類</dt><dd>ノート</dd>"));
    // R11: KB は変わらず、出力は git に入らない
    assert_eq!(f.read("kb/tech/notes/2026/2026-10-23-a.md"), note);
    assert_eq!(f.read(".cache/.gitignore"), "*\n");
}

#[test]
fn export_html_bundles_a_project_in_order_with_ended_items_folded() {
    let f = Fixture::new("export-html-bundle");
    f.write(
        "kb/projects/foo/README.md",
        "---\ntitle: foo\ntype: project\ndate: 2026-10-14\nprojects: [foo]\ntags: []\n---\n\n概要。[要件](specs/b.md#受け入れ) [知見](../../tech/insights/i.md)\n",
    );
    f.write(
        "kb/projects/foo/architecture.md",
        &doc("note", "技術構成", "2026-10-01", "", "構成"),
    );
    f.write(
        "kb/projects/foo/specs/a.md",
        &doc("spec", "下書きの要件", "2026-10-20", "draft", "a"),
    );
    f.write(
        "kb/projects/foo/specs/b.md",
        &doc(
            "spec",
            "承認済みの要件",
            "2026-10-10",
            "approved",
            "## 受け入れ\n\nb",
        ),
    );
    f.write(
        "kb/projects/foo/specs/c.md",
        &doc("spec", "やめた要件", "2026-10-05", "dropped", "c"),
    );
    f.write(
        "kb/tech/decisions/d1.md",
        &doc("decision", "案の決定", "2026-10-22", "proposed", "d"),
    );
    f.write(
        "kb/tech/decisions/d2.md",
        &doc("decision", "採用した決定", "2026-10-02", "accepted", "d"),
    );
    f.write(
        "kb/tech/decisions/d3.md",
        &doc("decision", "覆された決定", "2026-09-01", "superseded", "d"),
    );
    f.write(
        "kb/tech/insights/i.md",
        &doc("insight", "知見", "2026-10-11", "", "i"),
    );
    f.write(
        "kb/tech/notes/2026/n.md",
        &doc("note", "関係するノート", "2026-10-12", "", "n"),
    );
    f.write(
        "kb/projects/foo/sources/s.md",
        &doc("source", "取り込んだ資料", "2026-10-12", "", "s"),
    );

    let run = f.ika(&["export", "html", "--project", "foo"]);
    assert!(run.ok, "{}", run.err);
    let html = f.read(".cache/html/project-foo.html");
    let at = |title: &str| {
        html.find(&format!("<h2 class=\"doc-title\">{title}</h2>"))
            .unwrap_or_else(|| panic!("{title} がありません: {html}"))
    };
    // R2: README → 資料 → 要件書（approved → draft）→ 決定（accepted → proposed）→ 知見 → 終わったもの
    let order = [
        "foo",
        "技術構成",
        "承認済みの要件",
        "下書きの要件",
        "採用した決定",
        "案の決定",
        "知見",
        "やめた要件",
        "覆された決定",
    ];
    let positions: Vec<_> = order.iter().map(|t| at(t)).collect();
    assert!(positions.windows(2).all(|w| w[0] < w[1]), "{positions:?}");
    let ended = html.find("<details class=\"ended\"").unwrap();
    assert!(ended < at("やめた要件") && ended > at("知見"));
    // ノートは題名とリンクだけ、取り込んだ資料は入れない
    assert!(html.contains("<p class=\"part\" id=\"g-notes\">関連ノート</p>"));
    assert!(!html.contains("doc-title\">関係するノート"));
    assert!(!html.contains("取り込んだ資料"));
    // 束ねた中のリンクはアンカー（見出しへのリンクは、そのページの接頭辞つき。href の日本語はエンコードされる）
    assert!(
        html.contains("href=\"#d3-%E5%8F%97%E3%81%91%E5%85%A5%E3%82%8C\""),
        "{html}"
    );
    assert!(html.contains("<h3 id=\"d3-受け入れ\">"), "{html}");
    assert!(html.contains("href=\"#d7\""), "{html}");
}

#[test]
fn export_html_all_links_pages_relatively_and_applies_the_theme() {
    let f = Fixture::new("export-html-all");
    f.write(
        "kb/tech/notes/2026/2026-10-14-a.md",
        &NOTE.replace("本文", "[README](../../../projects/foo/README.md#foo)"),
    );
    f.write(
        "theme/style.css",
        ":root { --accent:#c00; } /* </style><script>x</script> */",
    );

    let run = f.ika(&["export", "html", "--all"]);
    assert!(run.ok, "{}", run.err);
    assert!(run.out.contains("2 ページを書き出しました"), "{}", run.out);
    let html = f.read(".cache/html/kb/tech/notes/2026/2026-10-14-a.html");
    assert!(
        html.contains("href=\"../../../projects/foo/README.html#foo\""),
        "{html}"
    );
    // R8: テーマは既定のあと。<style> を閉じられない
    let theme = html.find("--accent:#c00").unwrap();
    assert!(theme > html.find(".ikd-seq-grid").unwrap());
    assert!(!html.contains("</style><script>"));
    let index = f.read(".cache/html/index.html");
    assert!(
        index.contains("href=\"kb/tech/notes/2026/2026-10-14-a.html\""),
        "{index}"
    );
    assert!(f.exists(".cache/html/kb/projects/foo/README.html"));
}

#[test]
fn diagram_errors_stop_the_export_and_are_reported_by_check() {
    let f = Fixture::new("export-html-errors");
    f.write(
        "kb/tech/notes/2026/2026-10-14-a.md",
        &NOTE.replace("本文", "前\n\n```timeline\n先週: 何かした\n```\n"),
    );

    let run = f.ika(&["export", "html", "--all"]);
    assert!(!run.ok);
    assert!(
        run.err
            .contains("kb/tech/notes/2026/2026-10-14-a.md: 12行目: 図（timeline）: 日付は"),
        "{}",
        run.err
    );
    assert!(!f.exists(".cache/html/index.html"), "1ファイルも書かない");

    let check = f.ika(&["check"]);
    assert!(!check.ok);
    assert!(
        check
            .out
            .contains("kb/tech/notes/2026/2026-10-14-a.md: 12行目: 図（timeline）"),
        "{}",
        check.out
    );
}

#[test]
fn export_html_needs_exactly_one_target() {
    let f = Fixture::new("export-html-args");
    for args in [
        vec!["export", "html"],
        vec!["export", "html", "--all", "--project", "foo"],
        vec!["export", "html", "kb/projects/foo"],
        vec!["export", "html", "--project", "bar"],
        vec!["export", "html", "--all", "--with-kb"],
    ] {
        let run = f.ika(&args);
        assert!(!run.ok, "{args:?}");
    }
}

#[test]
fn serve_renders_diagrams_without_allowing_scripts() {
    let f = Fixture::new("serve-diagrams");
    f.write(
        "kb/tech/notes/2026/2026-10-14-a.md",
        &NOTE.replace(
            "本文",
            "```flow\na -> ?b\n  はい: +c\n```\n\n```seq\nA ->> B\n```\n",
        ),
    );
    f.write(
        "kb/projects/foo/sources/s.md",
        &doc(
            "source",
            "取り込んだ資料",
            "2026-10-12",
            "",
            "```seq\nA ->> B\n```",
        ),
    );
    let server = f.serve();
    let page = get(&server, "/kb/tech/notes/2026/2026-10-14-a.md");
    assert!(
        page.body.contains("<div class=\"ikd ikd-flow\">"),
        "{}",
        page.body
    );
    // 誤った図は、その場所に理由を出す
    assert!(page.body.contains("<div class=\"ikd-error\">"));
    assert!(!page.headers.contains("script-src"));
    // 取り込んだ資料の図は展開しない
    let source = get(&server, "/kb/projects/foo/sources/s.md");
    assert!(
        source.body.contains("<code class=\"language-seq\">"),
        "{}",
        source.body
    );
}

/// 語「共通語」がすべてのファイルに当たる KB。ノート `notes` 件と資料 `sources` 件。
/// どのファイルにも、語が 600 字目にある長い行（先頭）と、短い一致行 9 行がある
fn crowded(name: &str, notes: usize, sources: usize) -> Fixture {
    let f = Fixture::new(name);
    let doc = |kind: &str, i: usize| {
        let mut text = format!(
            "---\ntitle: t{i}\ntype: {kind}\ndate: 2026-10-{:02}\nprojects: [foo]\ntags: []\n---\n\n",
            1 + i % 28
        );
        text.push_str(&format!("{}共通語{}\n", "あ".repeat(600), "い".repeat(400)));
        for j in 0..9 {
            text.push_str(&format!("共通語を含む行 {i}-{j}\n"));
        }
        text
    };
    for i in 0..notes {
        f.write(
            &format!("kb/tech/notes/2026/2026-10-01-n{i:02}.md"),
            &doc("note", i),
        );
    }
    for i in 0..sources {
        f.write(
            &format!("kb/projects/foo/sources/s{i:02}.md"),
            &doc("source", i),
        );
    }
    f
}

fn file_lines(out: &str) -> Vec<&str> {
    out.lines().filter(|l| !l.starts_with(' ')).collect()
}

#[test]
fn search_caps_files_lines_and_line_length() {
    let f = crowded("search-caps", 35, 20);

    let run = f.ika(&["kb", "search", "共通語"]);
    assert!(run.ok, "{}", run.err);
    assert_eq!(file_lines(&run.out).len(), 20, "{}", run.out);
    assert!(
        run.err.contains("55 件（上位 20 件を表示。-n で増やせる）"),
        "{}",
        run.err
    );
    // 1 ファイル 3 行まで。残りは件数で知らせる
    assert_eq!(run.out.matches("  …ほか 7 行").count(), 20, "{}", run.out);
    assert_eq!(
        run.out
            .lines()
            .filter(|l| l.starts_with("  ") && l.contains(": "))
            .count(),
        60
    );
    // 長い行は語を残して切る
    let long = run.out.lines().find(|l| l.contains("あ")).unwrap();
    assert!(long.contains("共通語") && long.contains('…'), "{long}");
    assert!(long.chars().count() < 180, "{}", long.chars().count());

    let five = f.ika(&["kb", "search", "共通語", "-n", "5", "-l"]);
    assert_eq!(five.out.lines().count(), 5, "{}", five.out);
    assert!(five.err.contains("55 件（上位 5 件を表示"), "{}", five.err);

    // -l は既定で 50 件まで、-n 0 なら無制限
    let names = f.ika(&["kb", "search", "共通語", "-l"]);
    assert_eq!(names.out.lines().count(), 50, "{}", names.out);
    let all = f.ika(&["kb", "search", "共通語", "-n", "0"]);
    assert_eq!(file_lines(&all.out).len(), 55);
    assert_eq!(all.err.trim(), "55 件");
}

#[test]
fn search_without_terms_lists_up_to_fifty_files() {
    // 検索語がなければ一致行は出ず、1 件が短いので -l と同じ上限にする
    let f = crowded("search-no-terms", 55, 0);
    let run = f.ika(&["kb", "search", "-p", "foo"]);
    assert_eq!(file_lines(&run.out).len(), 50, "{}", run.out);
    assert!(run.err.contains("上位 50 件"), "{}", run.err);
}

#[test]
fn search_keeps_imported_sources() {
    let f = crowded("search-sources", 2, 2);
    let run = f.ika(&["kb", "search", "共通語", "-l"]);
    assert_eq!(run.out.lines().count(), 4, "{}", run.out);
    assert!(run.out.contains("kb/projects/foo/sources/s00.md"));
    let detailed = f.ika(&["kb", "search", "共通語", "--type", "source"]);
    assert_eq!(file_lines(&detailed.out).len(), 2, "{}", detailed.out);
    // 資料の一致行も 3 行まで
    assert_eq!(detailed.out.matches("  …ほか 7 行").count(), 2);
}

#[test]
fn cat_shows_only_the_outline_for_large_files() {
    let f = Fixture::new("cat-large");
    let line = "あ".repeat(100);
    let text = |n: usize| {
        format!(
            "---\ntitle: n\ntype: note\ndate: 2026-10-14\nprojects: [foo]\ntags: []\n---\n\n## 要点\n\n{}\n\n## 詳細\n\n詳細の本文\n",
            vec![line.as_str(); n].join("\n")
        )
    };
    let big = "kb/tech/notes/2026/2026-10-14-big.md";
    f.write(big, &text(90));
    let small = "kb/tech/notes/2026/2026-10-14-small.md";
    f.write(small, &text(70));

    let default = f.ika(&["kb", "cat", big]);
    assert!(default.ok, "{}", default.err);
    assert_eq!(default.out, f.ika(&["kb", "cat", big, "--outline"]).out);
    assert!(
        default.err.contains("大きいファイル") && default.err.contains("--full"),
        "{}",
        default.err
    );
    assert!(default.out.len() * 2 < f.read(big).len());

    assert_eq!(f.ika(&["kb", "cat", big, "--full"]).out, f.read(big));
    assert!(
        f.ika(&["kb", "cat", big, "--section", "詳細"])
            .out
            .contains("詳細の本文")
    );
    assert_eq!(f.ika(&["kb", "cat", small]).out, f.read(small));
    assert!(f.ika(&["kb", "cat", small]).err.is_empty());
    assert!(!f.ika(&["kb", "cat", big, "--full", "--outline"]).ok);
}

#[test]
fn project_show_caps_the_related_list() {
    let f = crowded("show-caps", 25, 0);
    let run = f.ika(&["project", "show", "foo"]);
    assert!(run.ok, "{}", run.err);
    assert!(run.out.contains("関連（25 件）"), "{}", run.out);
    assert_eq!(run.out.matches("      kb/").count(), 20, "{}", run.out);
    assert!(run.out.contains("…ほか 5 件（-n で増やせる）"));

    let all = f.ika(&["project", "show", "foo", "-n", "0"]);
    assert_eq!(all.out.matches("      kb/").count(), 25);
    assert!(!all.out.contains("…ほか"));
    let two = f.ika(&["project", "show", "foo", "-n", "2", "--readme"]);
    assert_eq!(two.out.matches("      kb/").count(), 2);
    assert!(two.out.contains("# foo"), "--readme の本文は全文のまま");
}

#[test]
fn default_outputs_stay_small() {
    // 回帰防止: 一般的な語でも、既定の出力がトークンを食いすぎない
    let f = crowded("output-size", 40, 20);
    let search = f.ika(&["kb", "search", "共通語"]);
    assert!(search.out.len() <= 20_000, "{} bytes", search.out.len());
    let names = f.ika(&["kb", "search", "共通語", "-l"]);
    assert!(names.out.len() <= 5_000, "{} bytes", names.out.len());
    let show = f.ika(&["project", "show", "foo"]);
    assert!(show.out.len() <= 8_000, "{} bytes", show.out.len());
}

/// リポジトリにあるプラグインのファイル
fn plugin_file(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../plugins/ika")
        .join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// SKILL.md のフロントマターの description
fn skill_description(text: &str) -> &str {
    text.lines()
        .find_map(|l| l.strip_prefix("description: "))
        .expect("description がある")
}

#[test]
fn skills_keep_ika_out_of_product_repos() {
    // ika を使わない人も同じリポジトリで開発するので、どのスキルも製品リポジトリに ika・KB の痕跡を残さない
    const RULE: &str = "製品リポジトリには ika・KB への言及を書かない";
    for skill in [
        "cycle",
        "search",
        "capture",
        "decide",
        "import",
        "project-sync",
        "project-add",
    ] {
        let text = plugin_file(&format!("skills/{skill}/SKILL.md"));
        assert!(text.contains(RULE), "{skill} に規則がない");
    }

    let cycle = plugin_file("skills/cycle/SKILL.md");
    assert!(
        !cycle.contains("Spec: kb/"),
        "コミットに要件書のパスを書かせない"
    );
    assert!(cycle.contains("references/product-repo.md"));

    // スキルを呼ばずに ika を使う作業でも見えるよう、常に読み込まれる description に入れる
    let search = plugin_file("skills/search/SKILL.md");
    let description = skill_description(&search);
    assert!(
        description.contains("ika・KB への言及を書かない"),
        "{description}"
    );
    assert!(description.chars().count() <= 1024);

    let guide = plugin_file("skills/cycle/references/product-repo.md");
    assert!(
        guide.contains("git diff --cached"),
        "ステージした差分の点検"
    );
    assert!(guide.contains("一次情報"), "出典は一次情報だけ");
    assert!(guide.contains("ika 自身のリポジトリ"), "例外");

    // 「出典を示す」は KB の中の話で、製品では KB を出典にしない
    for text in [
        plugin_file("skills/capture/SKILL.md"),
        fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../AGENTS.md")).unwrap(),
    ] {
        assert!(text.contains("KB を出典にしない"));
    }
}
