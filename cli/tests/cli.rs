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
        // stdout は Child に持たせたままにする。閉じると、サーバーが続きの行を出すときに SIGPIPE で死ぬ
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
    assert!(page.body.contains("<h2>要点</h2>") && page.body.contains("<li>箇条書き</li>"));
    assert!(page.body.contains("<table>"));
    assert!(page.body.contains("href=\"/project/foo\"") && page.body.contains("/search?tag=auth"));
    assert!(
        !page.body.contains("title: n"),
        "フロントマターは本文に出さない"
    );
    assert!(
        page.body.contains("このページへのリンク") && page.body.contains("/kb/tech/insights/i.md")
    );
}

#[test]
fn serve_resolves_relative_links_between_notes_in_the_browser() {
    // 相対リンクは URL の階層と KB の階層が同じなので、そのまま辿れる
    let f = Fixture::new("serve-links");
    f.write("kb/tech/notes/2026/2026-10-14-a.md", NOTE);
    f.write(
        "kb/tech/insights/i.md",
        &NOTE.replace("本文", "[根拠](../notes/2026/2026-10-14-a.md)"),
    );
    let server = f.serve();
    let page = get(&server, "/kb/tech/insights/i.md");
    assert!(
        page.body.contains("href=\"../notes/2026/2026-10-14-a.md\""),
        "{}",
        page.body
    );
    let target = get(&server, "/kb/tech/notes/2026/2026-10-14-a.md");
    assert_eq!(target.status, 200);
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
        project.body.matches("<h1>foo</h1>").count(),
        2,
        "見出しと、描画された README"
    );
    assert!(project.body.contains("2026-10-14-a.md"));
    assert_eq!(get(&server, "/project/nope").status, 404);

    let found = get(&server, "/search?q=foo");
    assert_eq!(found.status, 200);
    let decision = found.body.find("2026-10-15-d.md").unwrap();
    let note = found.body.find("2026-10-14-a.md").unwrap();
    assert!(decision < note, "採用済みの決定が先");
    assert!(found.body.contains("decision accepted"));
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
