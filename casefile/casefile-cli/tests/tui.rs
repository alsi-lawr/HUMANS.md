#![cfg(target_os = "linux")]

use std::{
    ffi::{OsStr, OsString},
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use tempfile::{TempDir, tempdir};

const EPIC: &str = "projects/demo/investigations/sample/epics/accepted/HMD-E-001.md";

struct Fixture {
    temporary: TempDir,
    root: PathBuf,
}

fn fixture() -> Fixture {
    let temporary = tempdir().expect("temporary root");
    let root = temporary.path().join("planning");
    fs::create_dir_all(&root).expect("temporary root");
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../casefile-store/tests/fixtures/minimum"),
        &root,
    );
    git(temporary.path(), &["init", "-q"]);
    git(temporary.path(), &["add", "."]);
    git(
        temporary.path(),
        &[
            "-c",
            "user.name=Casefile Test",
            "-c",
            "user.email=casefile@example.test",
            "commit",
            "-qm",
            "fixture",
        ],
    );
    Fixture { temporary, root }
}

fn git(root: &Path, args: &[&str]) {
    assert!(
        Command::new("git")
            .current_dir(root)
            .args(args)
            .status()
            .expect("git")
            .success()
    );
}

fn copy_tree(from: &Path, to: &Path) {
    for entry in fs::read_dir(from).expect("fixture entries") {
        let entry = entry.expect("fixture entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("fixture type").is_dir() {
            fs::create_dir_all(&target).expect("fixture directory");
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).expect("fixture file");
        }
    }
}

fn executable(path: &Path, source: &str) {
    fs::write(path, source).expect("fake program");
    let mut permissions = fs::metadata(path).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("permissions");
}

struct Pty {
    child: Child,
    input: ChildStdin,
    transcript: PathBuf,
}

impl Pty {
    fn start(fixture: &Fixture, args: &[OsString], environment: &[(&str, OsString)]) -> Self {
        let transcript = fixture.temporary.path().join("terminal.log");
        let mut command = format!(
            "stty rows 30 cols 120; exec '{}' --root '{}' tui",
            env!("CARGO_BIN_EXE_casefile"),
            fixture.root.display()
        );
        for arg in args {
            command += &format!(" '{}'", arg.to_string_lossy().replace('\'', "'\\''"));
        }
        let mut child = Command::new("script");
        child
            .args(["--quiet", "--flush", "--return", "--command", &command])
            .arg(&transcript)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        for (name, value) in environment {
            child.env(name, value);
        }
        let mut child = child.spawn().expect("PTY");
        Self {
            input: child.stdin.take().expect("PTY input"),
            child,
            transcript,
        }
    }

    fn send(&mut self, bytes: &[u8]) {
        self.input.write_all(bytes).expect("PTY input");
        self.input.flush().expect("PTY flush");
    }

    fn wait_for(&self, expected: &str) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            let content = fs::read(&self.transcript).unwrap_or_default();
            if content
                .windows(expected.len())
                .any(|part| part == expected.as_bytes())
            {
                return;
            }
            thread::sleep(Duration::from_millis(25));
        }
        panic!(
            "missing {expected:?} in {}",
            String::from_utf8_lossy(&fs::read(&self.transcript).unwrap_or_default())
        );
    }

    fn finish(self, success: bool) -> Vec<u8> {
        drop(self.input);
        let output = self.child.wait_with_output().expect("PTY output");
        assert_eq!(output.status.success(), success, "{:?}", output.stderr);
        fs::read(self.transcript).expect("transcript")
    }
}

fn editor_args(editor: &Path, values: &[&str]) -> Vec<OsString> {
    let mut args = vec!["--editor".into(), editor.as_os_str().to_owned()];
    for value in values {
        args.extend(["--editor-arg".into(), (*value).into()]);
    }
    args
}

fn begin_edit(pty: &mut Pty) {
    pty.wait_for("q quit");
    thread::sleep(Duration::from_millis(200));
    pty.send(b"\r\re");
}

fn restored(transcript: &[u8]) -> bool {
    let count = |sequence: &[u8]| {
        transcript
            .windows(sequence.len())
            .filter(|part| *part == sequence)
            .count()
    };
    let entered = count(b"\x1b[?1049h");
    entered > 0 && entered == count(b"\x1b[?1049l")
}

#[test]
fn editor_arg_requires_editor() {
    let output = Command::new(env!("CARGO_BIN_EXE_casefile"))
        .args(["tui", "--editor-arg", "orphan"])
        .output()
        .expect("casefile help");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--editor <PROGRAM>"));
}

#[test]
fn default_opener_ignores_editor_prompts_for_enter_and_cancels() {
    let fixture = fixture();
    let bin = fixture.temporary.path().join("bin");
    fs::create_dir(&bin).expect("fake bin");
    let log = fixture.temporary.path().join("opener.log");
    executable(
        &bin.join("xdg-open"),
        "#!/bin/sh\nprintf '%s\\n%s' \"$EDITOR\" \"$1\" > \"$CASEFILE_LOG\"\nsed -i 's/Minimum epic/Opened epic/' \"$1\"\n",
    );
    let mut path = bin.into_os_string();
    path.push(":");
    path.push(std::env::var_os("PATH").expect("PATH"));
    let environment = [
        ("PATH", path),
        ("EDITOR", "/invalid/editor".into()),
        ("CASEFILE_LOG", log.as_os_str().to_owned()),
    ];
    let mut pty = Pty::start(&fixture, &[], &environment);
    pty.wait_for("q quit");
    pty.send(b"\r\r");
    pty.send(b"?");
    pty.wait_for("Keyboard help");
    pty.send(b"?");
    thread::sleep(Duration::from_millis(100));
    pty.send(b"\t");
    thread::sleep(Duration::from_millis(100));
    pty.send(b"l");
    thread::sleep(Duration::from_millis(100));
    pty.send(&[b'j'; 35]);
    pty.wait_for("Verification");
    pty.send(b"e");
    pty.wait_for("press Enter to continue");
    let opened = fs::read_to_string(&log).expect("opener log");
    let lines: Vec<_> = opened.lines().collect();
    assert_eq!(lines[0], "/invalid/editor");
    let draft = Path::new(lines[1]);
    assert_eq!(draft.extension(), Some(OsStr::new("md")));
    pty.send(b"\n");
    pty.wait_for("REVIEW CHANGES");
    pty.send(b"c");
    pty.wait_for("Cancelled; discarded draft");
    thread::sleep(Duration::from_millis(100));
    pty.send(b"q");
    let transcript = pty.finish(true);
    assert!(
        fs::read_to_string(fixture.root.join(EPIC))
            .expect("epic")
            .contains("Minimum epic")
    );
    assert!(!draft.exists());
    assert!(String::from_utf8_lossy(&transcript).contains("Keyboard help"));
    assert!(String::from_utf8_lossy(&transcript).contains("Verification"));
    assert!(restored(&transcript));
}

#[test]
fn explicit_editor_preserves_arguments_applies_and_rescans() {
    let fixture = fixture();
    let editor = fixture.temporary.path().join("editor");
    let log = fixture.temporary.path().join("editor.log");
    executable(
        &editor,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$CASEFILE_LOG\"\nfor draft do :; done\nsed -i 's/Minimum epic/Edited epic/' \"$draft\"\n",
    );
    let environment = [("CASEFILE_LOG", log.as_os_str().to_owned())];
    let mut pty = Pty::start(
        &fixture,
        &editor_args(&editor, &["first", "two words"]),
        &environment,
    );
    begin_edit(&mut pty);
    pty.wait_for("REVIEW CHANGES");
    let logged = fs::read_to_string(log).expect("editor log");
    assert_eq!(
        &logged.lines().collect::<Vec<_>>()[..2],
        &["first", "two words"]
    );
    pty.send(b"a");
    pty.wait_for("Applied");
    thread::sleep(Duration::from_millis(100));
    pty.send(b"q");
    let transcript = pty.finish(true);
    assert!(
        fs::read_to_string(fixture.root.join(EPIC))
            .expect("epic")
            .contains("Edited epic")
    );
    assert!(String::from_utf8_lossy(&transcript).contains("rescanned revision"));
    assert!(restored(&transcript));
}

#[test]
fn stale_apply_preserves_concurrent_content_and_retains_draft() {
    let fixture = fixture();
    let editor = fixture.temporary.path().join("editor");
    let log = fixture.temporary.path().join("editor.log");
    executable(
        &editor,
        "#!/bin/sh\nprintf '%s' \"$1\" > \"$CASEFILE_LOG\"\nsed -i 's/Minimum epic/Edited epic/' \"$1\"\n",
    );
    let environment = [("CASEFILE_LOG", log.as_os_str().to_owned())];
    let mut pty = Pty::start(&fixture, &editor_args(&editor, &[]), &environment);
    begin_edit(&mut pty);
    pty.wait_for("REVIEW CHANGES");
    let canonical = fixture.root.join(EPIC);
    fs::write(
        &canonical,
        fs::read_to_string(&canonical)
            .expect("epic")
            .replace("Minimum epic", "Concurrent epic"),
    )
    .expect("concurrent edit");
    pty.send(b"a");
    let transcript = pty.finish(false);
    assert!(
        fs::read_to_string(canonical)
            .expect("epic")
            .contains("Concurrent epic")
    );
    assert!(Path::new(&fs::read_to_string(log).expect("editor log")).is_file());
    let transcript = String::from_utf8_lossy(&transcript);
    assert!(transcript.contains("stale store revision"));
    assert!(transcript.contains("canonical files unchanged; draft retained at"));
}
