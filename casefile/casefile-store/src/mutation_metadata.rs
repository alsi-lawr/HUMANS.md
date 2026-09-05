use crate::store::{StoreError, require_safe_target_parent};
use casefile_core::Kind;
use serde::Deserialize;
use std::{
    fs::{self, File},
    io::{BufRead, BufReader},
    path::Path,
};

#[derive(Default, Deserialize)]
pub(super) struct Header {
    pub(super) id: Option<String>,
    pub(super) phase: Option<String>,
    #[serde(default)]
    pub(super) decision_refs: Vec<String>,
    #[serde(default)]
    pub(super) related_tickets: Vec<String>,
    #[serde(default)]
    pub(super) supersedes: Vec<String>,
    #[serde(default)]
    pub(super) superseded_by: Vec<String>,
    #[serde(default)]
    pub(super) refs: Vec<String>,
    #[serde(default)]
    pub(super) attachments: Vec<String>,
}
impl Header {
    pub(super) fn references(&self) -> impl Iterator<Item = &String> {
        self.decision_refs
            .iter()
            .chain(&self.related_tickets)
            .chain(&self.supersedes)
            .chain(&self.superseded_by)
            .chain(&self.refs)
    }
}

pub(super) fn stem(path: &str) -> &str {
    path.rsplit('/')
        .next()
        .unwrap_or(path)
        .trim_end_matches(".md")
}

pub(super) fn from_bytes(bytes: &[u8], kind: Option<Kind>) -> Header {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Header::default();
    };
    if matches!(kind, Some(Kind::Evidence | Kind::Review)) {
        return casefile_core::parse_metadata_arrays("metadata", text)
            .map(|(refs, attachments)| Header {
                refs,
                attachments,
                ..Header::default()
            })
            .unwrap_or_default();
    }
    if matches!(kind, Some(Kind::Board | Kind::StrategyTransition)) {
        return toml::from_str(text).unwrap_or_default();
    }
    let Some(rest) = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
    else {
        return Header::default();
    };
    let Some((frontmatter, _)) = rest
        .split_once("\n---\n")
        .or_else(|| rest.split_once("\r\n---\r\n"))
    else {
        return Header::default();
    };
    serde_saphyr::from_str(frontmatter).unwrap_or_default()
}

pub(super) fn header(root: &Path, path: &str, kind: Kind) -> Result<Header, StoreError> {
    let target = root.join(path);
    let metadata = match fs::symlink_metadata(&target) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => metadata,
        Ok(_) => return Ok(Header::default()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Header::default()),
        Err(error) => return Err(error.into()),
    };
    let expected = crate::revision::metadata_revision(&target, &metadata)?;
    let file = match File::open(&target) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Header::default()),
        Err(error) => return Err(error.into()),
    };
    let opened = file.metadata()?;
    if !opened.is_file() || crate::revision::open_file_revision(&file, &opened)? != expected {
        return Ok(Header::default());
    }
    let mut bytes = Vec::new();
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {
                return Ok(Header::default());
            }
            Err(error) => return Err(error.into()),
        }
        if !matches!(kind, Kind::Board | Kind::StrategyTransition)
            && bytes.is_empty()
            && line.trim_end() != "---"
        {
            break;
        }
        let end = !matches!(kind, Kind::Board | Kind::StrategyTransition)
            && !bytes.is_empty()
            && line.trim_end() == "---";
        bytes.extend_from_slice(line.as_bytes());
        if matches!(kind, Kind::Board | Kind::StrategyTransition) {
            if let Ok(parsed) =
                toml::from_str::<Header>(std::str::from_utf8(&bytes).unwrap_or_default())
            {
                if parsed.id.is_some()
                    || (kind == Kind::StrategyTransition && parsed.phase.is_some())
                {
                    return Ok(parsed);
                }
            }
        }
        if end {
            break;
        }
    }
    Ok(from_bytes(&bytes, Some(kind)))
}

pub(super) fn list(
    root: &Path,
    directory: &str,
    recursive: bool,
) -> Result<Vec<String>, StoreError> {
    match require_safe_target_parent(root, Path::new(directory), "mutation discovery") {
        Ok(()) => {}
        Err(StoreError::Invalid(_)) => return Ok(Vec::new()),
        Err(error) => return Err(error),
    }
    let entries = match fs::read_dir(root.join(directory)) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry?;
        let path = format!("{directory}/{}", entry.file_name().to_string_lossy());
        let kind = entry.file_type()?;
        if kind.is_file() {
            paths.push(path);
        } else if kind.is_dir() && recursive {
            paths.extend(list(root, &path, true)?);
        }
    }
    Ok(paths)
}
