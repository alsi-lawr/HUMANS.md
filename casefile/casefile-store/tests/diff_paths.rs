use casefile_core::ChangeRequest;
use casefile_store::Store;
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

const TARGET: &str = "projects/demo/investigations/sample/tickets/accepted/HMD-011.md";

struct CurrentDirGuard(PathBuf);

impl CurrentDirGuard {
    fn enter(path: &Path) -> Self {
        let previous = env::current_dir().expect("current directory");
        env::set_current_dir(path).expect("enter temporary Git fixture");
        Self(previous)
    }
}

impl Drop for CurrentDirGuard {
    fn drop(&mut self) {
        env::set_current_dir(&self.0).expect("restore current directory");
    }
}

#[test]
fn relative_store_roots_create_real_git_diffs_with_relative_temp_arguments() {
    let temporary = tempfile::tempdir().expect("temporary root");
    let root = temporary.path().join("Store");
    fs::create_dir(&root).expect("Store directory");
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimum"),
        &root,
    );
    for args in [
        &["init", "-q"][..],
        &["config", "user.email", "casefile@example.test"],
        &["config", "user.name", "Casefile Test"],
        &["add", "."],
        &["commit", "-qm", "fixture"],
    ] {
        assert!(
            Command::new("git")
                .current_dir(&root)
                .args(args)
                .status()
                .expect("git fixture")
                .success()
        );
    }

    let named_diff = {
        let _guard = CurrentDirGuard::enter(temporary.path());
        preview_delete(Path::new("Store"))
    };
    let dot_diff = {
        let _guard = CurrentDirGuard::enter(&root);
        preview_delete(Path::new("."))
    };
    for diff in [named_diff, dot_diff] {
        assert!(diff.contains(&format!("diff --git a/{TARGET} b/{TARGET}")));
        assert!(diff.contains(&format!("--- a/{TARGET}")));
        assert!(diff.contains("+++ /dev/null"));
        assert!(!diff.contains(".tmp"), "{diff}");
    }
}

fn preview_delete(root: &Path) -> String {
    Store::open(root)
        .expect("relative Store")
        .preview(ChangeRequest::Delete {
            path: TARGET.into(),
        })
        .expect("relative-root preview")
        .diff
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
