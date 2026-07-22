use crate::workbench::App;
use casefile_core::{
    CasefileSnapshot, Classification, Diagnostic, EntrySnapshot, Kind, RecordSummary, Revision,
};
use casefile_store::{ActivationState, ScanResult};
use ratatui::{Terminal, backend::TestBackend};
use std::collections::BTreeMap;

pub(crate) fn entry(
    path: &str,
    classification: Classification,
    kind: Option<Kind>,
    summary: Option<RecordSummary>,
    bytes: &[u8],
) -> EntrySnapshot {
    EntrySnapshot {
        path: path.into(),
        classification,
        kind,
        identity: summary.as_ref().and_then(|summary| match summary {
            RecordSummary::WorkItem { id, .. } | RecordSummary::Board { id, .. } => {
                Some(id.clone())
            }
            _ => None,
        }),
        content_revision: Revision("sha256:entry".into()),
        summary,
        original_bytes: bytes.into(),
    }
}

pub(crate) fn scan() -> ScanResult {
    ScanResult {
        activation: ActivationState::Active,
        investigation_roots: BTreeMap::from([("demo".into(), vec!["sample".into()])]),
        snapshot: CasefileSnapshot {
            revision: Revision("sha256:scan".into()),
            entries: vec![
                entry(
                    "projects/demo/investigations/sample/tickets/accepted/HMD-013.md",
                    Classification::Governed,
                    Some(Kind::Ticket),
                    Some(RecordSummary::WorkItem {
                        id: "HMD-013".into(),
                        title: "Navigator".into(),
                        status: "accepted".into(),
                        rank: Some(3),
                    }),
                    b"# Navigator\n\n- **first** line\n- `second` line\n\n| Safe | Value |\n| --- | --- |\n| yes | 1 |\n\n\x1b[31mnot a colour",
                ),
                entry(
                    "projects/demo/investigations/sample/boards/main.toml",
                    Classification::Governed,
                    Some(Kind::Board),
                    Some(RecordSummary::Board {
                        id: "HMD-board".into(),
                        title: "Board".into(),
                        columns: vec!["Ready".into(), "Done".into()],
                    }),
                    b"board",
                ),
                entry(
                    "projects/demo/investigations/sample/evidence/c-legacy.txt",
                    Classification::Ungoverned,
                    None,
                    None,
                    b"legacy",
                ),
                entry(
                    "projects/demo/investigations/sample/review/d-invalid.md",
                    Classification::Invalid,
                    Some(Kind::Ticket),
                    None,
                    &[0xff, 0x00, 0x10],
                ),
                entry(
                    "projects/demo/decision-log/e-raw.txt",
                    Classification::Raw,
                    None,
                    None,
                    b"raw",
                ),
            ],
        },
        diagnostics: vec![
            Diagnostic::new(
                "projects/demo/investigations/sample/review/d-invalid.md",
                "invalid_shape",
                "ticket is incomplete",
            ),
            Diagnostic::new(
                "projects/demo/investigations/sample/tickets/accepted/HMD-013.md",
                "cross_record",
                "separate scanner channel",
            ),
        ],
    }
}

pub(crate) fn render(app: &App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| app.render(frame.area(), frame.buffer_mut()))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
