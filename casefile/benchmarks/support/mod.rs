use std::{fs, path::Path, process::Command};
use tempfile::TempDir;

pub const INVESTIGATION: &str = "projects/demo/investigations/sample";

pub struct Fixture {
    pub root: TempDir,
}

impl Fixture {
    pub fn new() -> Self {
        let root = TempDir::new().expect("benchmark root");
        copy_tree(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../casefile-store/tests/fixtures/minimum"),
            root.path(),
        );
        Self { root }
    }

    pub fn opaque_files(self, count: usize) -> Self {
        let directory = self.root.path().join("raw");
        fs::create_dir(&directory).expect("raw directory");
        for number in 0..count {
            fs::write(directory.join(format!("{number:06}.txt")), b"opaque\n").expect("raw file");
        }
        self
    }

    pub fn activation_roots(self, count: usize) -> Self {
        assert!(count >= 1);
        let roots = std::iter::once(INVESTIGATION.to_owned())
            .chain(
                (1..count).map(|number| format!("projects/demo/investigations/other-{number:04}")),
            )
            .map(|root| format!("\"{root}\""))
            .collect::<Vec<_>>()
            .join(", ");
        fs::write(
            self.root.path().join("casefile.toml"),
            format!("schema_version = 1\n\n[projects.demo]\nprefix = \"HMD\"\ninvestigations = [{roots}]\n"),
        )
        .expect("activation");
        self
    }

    pub fn tickets(self, count: usize, chain: bool) -> Self {
        let directory = self
            .root
            .path()
            .join(INVESTIGATION)
            .join("tickets/accepted");
        let template = fs::read_to_string(directory.join("HMD-011.md")).expect("ticket template");
        for number in 0..count {
            let id = format!("HMD-{:06}", number + 100_000);
            let previous = (number > 0).then(|| format!("HMD-{:06}", number + 99_999));
            let next = (number + 1 < count).then(|| format!("HMD-{:06}", number + 100_001));
            let supersedes = if chain {
                previous.map_or_else(|| "[]".to_owned(), |id| format!("[{id}]"))
            } else {
                "[]".to_owned()
            };
            let superseded_by = if chain {
                next.map_or_else(|| "[]".to_owned(), |id| format!("[{id}]"))
            } else {
                "[]".to_owned()
            };
            let text = template
                .replace("HMD-011", &id)
                .replace("supersedes: []", &format!("supersedes: {supersedes}"))
                .replace(
                    "superseded_by: []",
                    &format!("superseded_by: {superseded_by}"),
                );
            fs::write(directory.join(format!("{id}.md")), text).expect("ticket");
        }
        self
    }

    pub fn progress_notes(self, count: usize, missing_targets: bool) -> Self {
        let directory = self.root.path().join(INVESTIGATION).join("progress");
        fs::create_dir_all(&directory).expect("progress directory");
        let mut log = String::from("schema_version = 1\n");
        for number in 0..count {
            let ticket = if missing_targets {
                format!("HMD-{:06}", number + 800_000)
            } else {
                "HMD-011".to_owned()
            };
            log.push_str(&format!("\n[[entries]]\nid = \"note-{number:06}\"\nrecorded_at = \"2026-07-26T10:01:00Z\"\nrecorded_by = \"benchmark\"\nticket_id = \"{ticket}\"\nkind = \"note\"\ncategory = \"quirk\"\nmessage = \"Synthetic benchmark note.\"\n"));
        }
        fs::write(directory.join("log.toml"), log).expect("progress log");
        self
    }

    pub fn long_ticket_body(self, paragraphs: usize) -> Self {
        let path = self
            .root
            .path()
            .join(INVESTIGATION)
            .join("tickets/accepted/HMD-011.md");
        let text = fs::read_to_string(&path).expect("ticket body");
        let body = (0..paragraphs)
            .map(|number| {
                format!("Synthetic detail paragraph {number}, for repeatable scrolling.\n\n")
            })
            .collect::<String>();
        fs::write(path, text.replace("Required.", &body)).expect("long ticket body");
        self
    }

    pub fn git_repository(self) -> Self {
        for arguments in [
            &["init", "-q"][..],
            &["config", "user.email", "benchmark@example.test"],
            &["config", "user.name", "Casefile Benchmark"],
            &["add", "."],
            &["commit", "-qm", "benchmark fixture"],
        ] {
            assert!(
                Command::new("git")
                    .current_dir(self.root.path())
                    .args(arguments)
                    .status()
                    .expect("git")
                    .success()
            );
        }
        self
    }
}

fn copy_tree(from: &Path, to: &Path) {
    for entry in fs::read_dir(from).expect("fixture entries") {
        let entry = entry.expect("fixture entry");
        let destination = to.join(entry.file_name());
        if entry.file_type().expect("fixture file type").is_dir() {
            fs::create_dir_all(&destination).expect("fixture directory");
            copy_tree(&entry.path(), &destination);
        } else {
            fs::copy(entry.path(), destination).expect("fixture file");
        }
    }
}
