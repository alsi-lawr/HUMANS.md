use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub schema_version: u32,
    pub code: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    pub message: String,
}

impl Diagnostic {
    pub fn new(path: impl Into<String>, code: &str, message: impl Into<String>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            code: code.into(),
            path: path.into(),
            field: None,
            section: None,
            message: message.into(),
        }
    }

    pub fn field(mut self, field: &str) -> Self {
        self.field = Some(field.into());
        self
    }

    pub fn section(mut self, section: &str) -> Self {
        self.section = Some(section.into());
        self
    }
}

pub fn stable(mut diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    diagnostics.sort_by(|a, b| {
        (&a.path, &a.code, &a.field, &a.section, &a.message)
            .cmp(&(&b.path, &b.code, &b.field, &b.section, &b.message))
    });
    diagnostics
}
