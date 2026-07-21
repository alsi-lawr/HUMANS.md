#![cfg(target_os = "linux")]

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
    thread,
    time::Duration,
};

struct TemporaryRoot(PathBuf);

impl Drop for TemporaryRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn fixture() -> TemporaryRoot {
    static NEXT_ROOT: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "casefile-cli-tui-{}-{}",
        std::process::id(),
        NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&root).expect("temporary root");
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../casefile-store/tests/fixtures/minimum"),
        &root,
    );
    TemporaryRoot(root)
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

#[test]
fn tui_help_scroll_and_quit_work_in_a_pty_and_restore_the_screen() {
    let root = fixture();
    let transcript = root.0.join("terminal.log");
    let command = format!(
        "stty rows 30 cols 120; exec {} --root {} tui",
        env!("CARGO_BIN_EXE_casefile"),
        root.0.display()
    );
    let mut process = Command::new("script")
        .args(["--quiet", "--return", "--command", &command])
        .arg(&transcript)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("script PTY utility");
    let mut input = process.stdin.take().expect("script stdin");
    thread::sleep(Duration::from_millis(200));
    input.write_all(b"?").expect("help key");
    thread::sleep(Duration::from_millis(150));
    input.write_all(b"?").expect("close help");
    thread::sleep(Duration::from_millis(100));
    input.write_all(b"\t").expect("focus key");
    thread::sleep(Duration::from_millis(100));
    input.write_all(b"l").expect("content tab key");
    thread::sleep(Duration::from_millis(150));
    input.write_all(&[b'j'; 35]).expect("scroll keys");
    thread::sleep(Duration::from_millis(150));
    input.write_all(b"q").expect("quit key");
    drop(input);
    let output = process.wait_with_output().expect("PTY process");
    assert!(output.status.success(), "{:?}", output.stderr);
    let transcript = fs::read(transcript).expect("transcript");
    assert!(
        transcript
            .windows(b"Keyboard help".len())
            .any(|bytes| bytes == b"Keyboard help")
    );
    assert!(
        transcript
            .windows(b"Verification".len())
            .any(|bytes| bytes == b"Verification")
    );
    assert!(transcript.windows(8).any(|bytes| bytes == b"\x1b[?1049h"));
    assert!(transcript.windows(8).any(|bytes| bytes == b"\x1b[?1049l"));
}

#[test]
fn tui_help_discovers_editor_flags() {
    let output = Command::new(env!("CARGO_BIN_EXE_casefile"))
        .args(["tui", "--help"])
        .output()
        .expect("tui help");
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).expect("UTF-8 help");
    assert!(help.contains("--editor <PROGRAM>"));
    assert!(help.contains("--editor-arg <ARG>"));
}
