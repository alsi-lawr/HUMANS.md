use casefile_core::Revision;
use casefile_store::{
    DerivedBoard, DerivedIndex, DerivedRecord, DerivedRelationship, DerivedSnapshot, Indexed,
    RecordScope, ScopedIdentity,
};
use rusqlite::{Connection, params};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SqliteIndexError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("SQLite error: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("index path must be outside the planning root")]
    InsidePlanningRoot,
}

pub struct SqliteIndex {
    path: PathBuf,
}

impl SqliteIndex {
    pub fn open(
        index_path: impl Into<PathBuf>,
        planning_root: &Path,
    ) -> Result<Self, SqliteIndexError> {
        let path = index_path.into();
        let root = fs::canonicalize(planning_root)?;
        let parent = path.parent().ok_or(SqliteIndexError::InsidePlanningRoot)?;
        let parent = fs::canonicalize(parent)?;
        if parent.starts_with(root) {
            return Err(SqliteIndexError::InsidePlanningRoot);
        }
        Ok(Self { path })
    }

    fn revision(&self) -> Result<Option<Revision>, SqliteIndexError> {
        if !self.path.exists() {
            return Ok(None);
        }
        let connection = Connection::open(&self.path)?;
        Ok(Some(Revision(connection.query_row(
            "SELECT source_revision FROM metadata LIMIT 1",
            [],
            |row| row.get(0),
        )?)))
    }

    fn checked<T>(
        &self,
        current: &Revision,
        read: impl FnOnce(&Connection) -> Result<T, SqliteIndexError>,
    ) -> Result<Indexed<T>, SqliteIndexError> {
        match self.state(current)? {
            Indexed::Current {
                source_revision, ..
            } => Ok(Indexed::Current {
                source_revision,
                value: read(&Connection::open(&self.path)?)?,
            }),
            Indexed::Missing => Ok(Indexed::Missing),
            Indexed::Stale {
                indexed_revision,
                current_revision,
            } => Ok(Indexed::Stale {
                indexed_revision,
                current_revision,
            }),
            Indexed::Corrupt { message } => Ok(Indexed::Corrupt { message }),
        }
    }
}

impl DerivedIndex for SqliteIndex {
    type Prepared = (NamedTempFile, Revision);
    type Error = SqliteIndexError;

    fn prepare(&self, snapshot: &DerivedSnapshot) -> Result<Self::Prepared, SqliteIndexError> {
        let parent = self
            .path
            .parent()
            .ok_or(SqliteIndexError::InsidePlanningRoot)?;
        let file = NamedTempFile::new_in(parent)?;
        let mut connection = Connection::open(file.path())?;
        connection.execute_batch("PRAGMA journal_mode=DELETE;
            CREATE TABLE metadata (source_revision TEXT NOT NULL);
            CREATE TABLE records (path TEXT PRIMARY KEY, project TEXT, investigation TEXT, identity TEXT, classification TEXT NOT NULL, kind TEXT, title TEXT NOT NULL, search_text TEXT NOT NULL, document TEXT NOT NULL);
            CREATE TABLE relationships (source_project TEXT NOT NULL, source_investigation TEXT, source_identity TEXT NOT NULL, target_project TEXT NOT NULL, target_investigation TEXT, target_identity TEXT NOT NULL, kind TEXT NOT NULL, document TEXT NOT NULL);
            CREATE TABLE boards (project TEXT NOT NULL, investigation TEXT, identity TEXT NOT NULL, title TEXT NOT NULL, document TEXT NOT NULL);")?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO metadata VALUES (?)",
            [&snapshot.source_revision.0],
        )?;
        for record in &snapshot.records {
            let (project, investigation) = record
                .scope
                .as_ref()
                .map(|value| (Some(value.project.as_str()), value.investigation.as_deref()))
                .unwrap_or((None, None));
            let identity = record
                .identity
                .as_ref()
                .map(|value| value.identity.as_str());
            transaction.execute(
                "INSERT INTO records VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    record.path,
                    project,
                    investigation,
                    identity,
                    format!("{:?}", record.classification),
                    record.kind.map(|value| format!("{:?}", value)),
                    record.title,
                    record.search_text,
                    serde_json::to_string(record)?
                ],
            )?;
        }
        for relationship in &snapshot.relationships {
            transaction.execute(
                "INSERT INTO relationships VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    relationship.source.scope.project,
                    relationship.source.scope.investigation,
                    relationship.source.identity,
                    relationship.target.scope.project,
                    relationship.target.scope.investigation,
                    relationship.target.identity,
                    format!("{:?}", relationship.kind),
                    serde_json::to_string(relationship)?
                ],
            )?;
        }
        for board in &snapshot.boards {
            transaction.execute(
                "INSERT INTO boards VALUES (?, ?, ?, ?, ?)",
                params![
                    board.identity.scope.project,
                    board.identity.scope.investigation,
                    board.identity.identity,
                    board.title,
                    serde_json::to_string(board)?
                ],
            )?;
        }
        transaction.commit()?;
        drop(connection);
        Ok((file, snapshot.source_revision.clone()))
    }

    fn publish(
        &self,
        prepared: Self::Prepared,
        current: &Revision,
    ) -> Result<Indexed<()>, SqliteIndexError> {
        if &prepared.1 != current {
            return Ok(Indexed::Stale {
                indexed_revision: prepared.1,
                current_revision: current.clone(),
            });
        }
        prepared
            .0
            .persist(&self.path)
            .map_err(|error| error.error)?;
        Ok(Indexed::Current {
            source_revision: current.clone(),
            value: (),
        })
    }

    fn state(&self, current: &Revision) -> Result<Indexed<()>, SqliteIndexError> {
        match self.revision() {
            Ok(None) => Ok(Indexed::Missing),
            Ok(Some(indexed)) if indexed == *current => Ok(Indexed::Current {
                source_revision: indexed,
                value: (),
            }),
            Ok(Some(indexed)) => Ok(Indexed::Stale {
                indexed_revision: indexed,
                current_revision: current.clone(),
            }),
            Err(error) => Ok(Indexed::Corrupt {
                message: error.to_string(),
            }),
        }
    }

    fn record(
        &self,
        current: &Revision,
        identity: &ScopedIdentity,
    ) -> Result<Indexed<Option<DerivedRecord>>, SqliteIndexError> {
        self.checked(current, |connection| {
            let mut statement = connection.prepare("SELECT document FROM records WHERE project = ? AND investigation IS ? AND identity = ?")?;
            let mut rows = statement.query(params![identity.scope.project, identity.scope.investigation, identity.identity])?;
            rows.next()?.map(|row| serde_json::from_str(&row.get::<_, String>(0)?).map_err(Into::into)).transpose()
        })
    }

    fn records(
        &self,
        current: &Revision,
        scope: Option<&RecordScope>,
        search: Option<&str>,
    ) -> Result<Indexed<Vec<DerivedRecord>>, SqliteIndexError> {
        self.checked(current, |connection| {
            let mut statement = connection.prepare("SELECT document FROM records ORDER BY path")?;
            let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
            let mut records = rows
                .map(|row| Ok(serde_json::from_str::<DerivedRecord>(&row?)?))
                .collect::<Result<Vec<_>, SqliteIndexError>>()?;
            records.retain(|record| {
                scope.is_none_or(|scope| {
                    record
                        .scope
                        .as_ref()
                        .is_some_and(|record_scope| record_scope == scope)
                }) && search.is_none_or(|text| {
                    record
                        .search_text
                        .to_lowercase()
                        .contains(&text.to_lowercase())
                })
            });
            Ok(records)
        })
    }

    fn relationships(
        &self,
        current: &Revision,
        identity: &ScopedIdentity,
    ) -> Result<Indexed<Vec<DerivedRelationship>>, SqliteIndexError> {
        self.checked(current, |connection| {
            let mut statement = connection.prepare("SELECT document FROM relationships WHERE (source_project = ? AND source_investigation IS ? AND source_identity = ?) OR (target_project = ? AND target_investigation IS ? AND target_identity = ?) ORDER BY kind, source_identity, target_identity")?;
            let rows = statement.query_map(params![identity.scope.project, identity.scope.investigation, identity.identity, identity.scope.project, identity.scope.investigation, identity.identity], |row| row.get::<_, String>(0))?;
            rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect::<Result<Vec<_>, SqliteIndexError>>()
        })
    }

    fn boards(
        &self,
        current: &Revision,
        scope: &ScopedIdentity,
    ) -> Result<Indexed<Vec<DerivedBoard>>, SqliteIndexError> {
        self.checked(current, |connection| {
            let mut statement = connection.prepare("SELECT document FROM boards WHERE project = ? AND investigation IS ? ORDER BY identity")?;
            let rows = statement.query_map(params![scope.scope.project, scope.scope.investigation], |row| row.get::<_, String>(0))?;
            rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect::<Result<Vec<_>, SqliteIndexError>>()
        })
    }
}
