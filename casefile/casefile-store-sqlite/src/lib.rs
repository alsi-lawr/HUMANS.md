use casefile_core::{Diagnostic, Revision};
use casefile_store::{
    DerivedBoard, DerivedIndex, DerivedRecord, DerivedRelationship, DerivedSnapshot, Indexed,
    RecordScope, RevisionSource, ScopedIdentity, StoreError,
};
use rusqlite::{Connection, ToSql, params, params_from_iter};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;
use thiserror::Error;

const SCHEMA_VERSION: i64 = 1;

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
    #[error("canonical revision check failed: {0}")]
    Revision(#[from] StoreError),
    #[error("record identity is ambiguous across paths (first two): {paths:?}")]
    AmbiguousRecordIdentity { paths: [String; 2] },
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

    fn checked<T>(
        &self,
        current: &Revision,
        read: impl FnOnce(&Connection) -> Result<T, SqliteIndexError>,
    ) -> Result<Indexed<T>, SqliteIndexError> {
        if !self.path.exists() {
            return Ok(Indexed::Missing);
        }
        let connection = Connection::open(&self.path)?;
        let indexed = connection.query_row(
            "SELECT source_revision, user_version FROM metadata CROSS JOIN pragma_user_version LIMIT 1",
            [],
            |row| {
                if row.get::<_, i64>(1)? != SCHEMA_VERSION {
                    return Ok(None);
                }
                Ok(Some(Revision(row.get(0)?)))
            },
        )?;
        let Some(indexed) = indexed else {
            return Ok(Indexed::Missing);
        };
        if indexed != *current {
            return Ok(Indexed::Stale {
                indexed_revision: indexed,
                current_revision: current.clone(),
            });
        }
        Ok(Indexed::Current {
            source_revision: indexed,
            value: read(&connection)?,
        })
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
        connection.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        connection.execute_batch("PRAGMA journal_mode=DELETE;
            CREATE TABLE metadata (source_revision TEXT NOT NULL);
            CREATE TABLE records (path TEXT PRIMARY KEY, project TEXT, investigation TEXT, identity TEXT, classification TEXT NOT NULL, kind TEXT, title TEXT NOT NULL, search_text TEXT NOT NULL, document TEXT NOT NULL);
            CREATE TABLE relationships (source_project TEXT NOT NULL, source_investigation TEXT, source_identity TEXT NOT NULL, target_project TEXT NOT NULL, target_investigation TEXT, target_identity TEXT NOT NULL, kind TEXT NOT NULL, document TEXT NOT NULL);
            CREATE TABLE boards (project TEXT NOT NULL, investigation TEXT, identity TEXT NOT NULL, title TEXT NOT NULL, document TEXT NOT NULL);
            CREATE TABLE diagnostics (path TEXT NOT NULL, code TEXT NOT NULL, document TEXT NOT NULL);")?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO metadata VALUES (?)",
            [&snapshot.source_revision.0],
        )?;
        {
            let mut insert =
                transaction.prepare("INSERT INTO records VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
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
                insert.execute(params![
                    record.path,
                    project,
                    investigation,
                    identity,
                    format!("{:?}", record.classification),
                    record.kind.map(|value| format!("{:?}", value)),
                    record.title,
                    record.search_text().to_lowercase(),
                    serde_json::to_string(record)?
                ])?;
            }
        }
        {
            let mut insert =
                transaction.prepare("INSERT INTO relationships VALUES (?, ?, ?, ?, ?, ?, ?, ?)")?;
            for relationship in &snapshot.relationships {
                insert.execute(params![
                    relationship.source.scope.project,
                    relationship.source.scope.investigation,
                    relationship.source.identity,
                    relationship.target.scope.project,
                    relationship.target.scope.investigation,
                    relationship.target.identity,
                    format!("{:?}", relationship.kind),
                    serde_json::to_string(relationship)?
                ])?;
            }
        }
        {
            let mut insert = transaction.prepare("INSERT INTO boards VALUES (?, ?, ?, ?, ?)")?;
            for board in &snapshot.boards {
                insert.execute(params![
                    board.identity.scope.project,
                    board.identity.scope.investigation,
                    board.identity.identity,
                    board.title,
                    serde_json::to_string(board)?
                ])?;
            }
        }
        {
            let mut insert = transaction.prepare("INSERT INTO diagnostics VALUES (?, ?, ?)")?;
            for diagnostic in &snapshot.diagnostics {
                insert.execute(params![
                    diagnostic.path,
                    diagnostic.code,
                    serde_json::to_string(diagnostic)?
                ])?;
            }
        }
        transaction.execute_batch("CREATE INDEX records_scope_path ON records (project, investigation, path);
            CREATE INDEX records_identity ON records (project, investigation, identity, path);
            CREATE INDEX relationships_source ON relationships (source_project, source_investigation, source_identity);
            CREATE INDEX relationships_target ON relationships (target_project, target_investigation, target_identity);
            CREATE INDEX boards_scope ON boards (project, investigation, identity);")?;
        transaction.commit()?;
        drop(connection);
        Ok((file, snapshot.source_revision.clone()))
    }

    fn publish(
        &self,
        prepared: Self::Prepared,
        source: &dyn RevisionSource,
    ) -> Result<Indexed<()>, SqliteIndexError> {
        let current = source.current_revision()?;
        if prepared.1 != current {
            return Ok(Indexed::Stale {
                indexed_revision: prepared.1,
                current_revision: current,
            });
        }
        prepared
            .0
            .persist(&self.path)
            .map_err(|error| error.error)?;
        Ok(Indexed::Current {
            source_revision: current,
            value: (),
        })
    }

    fn state(&self, current: &Revision) -> Result<Indexed<()>, SqliteIndexError> {
        self.checked(current, |_| Ok(()))
    }

    fn record(
        &self,
        current: &Revision,
        identity: &ScopedIdentity,
    ) -> Result<Indexed<Option<DerivedRecord>>, SqliteIndexError> {
        self.checked(current, |connection| {
            let mut statement = connection.prepare("SELECT path, document FROM records WHERE project = ? AND investigation IS ? AND identity = ? ORDER BY path LIMIT 2")?;
            let mut rows = statement.query(params![identity.scope.project, identity.scope.investigation, identity.identity])?;
            let Some(first) = rows.next()? else { return Ok(None); };
            let path: String = first.get(0)?;
            let document: String = first.get(1)?;
            if let Some(second) = rows.next()? {
                return Err(SqliteIndexError::AmbiguousRecordIdentity { paths: [path, second.get(0)?] });
            }
            Ok(Some(serde_json::from_str(&document)?))
        })
    }

    fn records(
        &self,
        current: &Revision,
        scope: Option<&RecordScope>,
        search: Option<&str>,
    ) -> Result<Indexed<Vec<DerivedRecord>>, SqliteIndexError> {
        self.checked(current, |connection| {
            let search = search.map(str::to_lowercase);
            let mut sql = String::from("SELECT document FROM records");
            let mut parameters: Vec<&dyn ToSql> = Vec::new();
            if let Some(scope) = scope {
                sql.push_str(" WHERE project = ? AND investigation IS ?");
                parameters.extend([&scope.project as &dyn ToSql, &scope.investigation]);
            }
            if let Some(search) = &search {
                sql.push_str(if scope.is_some() { " AND" } else { " WHERE" });
                sql.push_str(" instr(search_text, ?) > 0");
                parameters.push(search);
            }
            sql.push_str(" ORDER BY path");
            let mut statement = connection.prepare(&sql)?;
            let rows =
                statement.query_map(params_from_iter(parameters), |row| row.get::<_, String>(0))?;
            rows.map(|row| Ok(serde_json::from_str::<DerivedRecord>(&row?)?))
                .collect::<Result<Vec<_>, SqliteIndexError>>()
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

    fn diagnostics(
        &self,
        current: &Revision,
    ) -> Result<Indexed<Vec<Diagnostic>>, SqliteIndexError> {
        self.checked(current, |connection| {
            let mut statement =
                connection.prepare("SELECT document FROM diagnostics ORDER BY path, code")?;
            let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
            rows.map(|row| Ok(serde_json::from_str(&row?)?))
                .collect::<Result<Vec<_>, SqliteIndexError>>()
        })
    }

    fn boards(
        &self,
        current: &Revision,
        scope: &RecordScope,
    ) -> Result<Indexed<Vec<DerivedBoard>>, SqliteIndexError> {
        self.checked(current, |connection| {
            let mut statement = connection.prepare("SELECT document FROM boards WHERE project = ? AND investigation IS ? ORDER BY identity")?;
            let rows = statement.query_map(params![scope.project, scope.investigation], |row| row.get::<_, String>(0))?;
            rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect::<Result<Vec<_>, SqliteIndexError>>()
        })
    }
}
