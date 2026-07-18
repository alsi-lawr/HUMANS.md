#![cfg(target_os = "linux")]

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
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
fn tui_quits_from_a_pty_and_restores_the_alternate_screen() {
    let root = fixture();
    let transcript = root.0.join("terminal.log");
    let command = format!(
        "{} --root {} tui",
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
    use std::io::Write;
    process
        .stdin
        .take()
        .expect("script stdin")
        .write_all(b"q")
        .expect("quit key");
    let output = process.wait_with_output().expect("PTY process");
    assert!(output.status.success(), "{:?}", output.stderr);
    let transcript = fs::read(transcript).expect("transcript");
    assert!(transcript.windows(8).any(|bytes| bytes == b"\x1b[?1049h"));
    assert!(transcript.windows(8).any(|bytes| bytes == b"\x1b[?1049l"));
}
