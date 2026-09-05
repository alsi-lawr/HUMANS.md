use crate::store::StoreError;
use casefile_core::Revision;
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

const METADATA_REVISION_VERSION: &[u8] = b"casefile-fs-metadata-v1\0";
const STORE_REVISION_VERSION: &[u8] = b"casefile-fs-metadata-tree-v1\0";
const SYNTHETIC_REVISION_VERSION: &[u8] = b"casefile-synthetic-overlay-v1\0";

pub(super) fn target_revision(path: &Path) -> Result<Option<Revision>, StoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => metadata_revision(path, &metadata).map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn require_target_revision(
    path: &Path,
    expected: Option<&Revision>,
) -> Result<(), StoreError> {
    let current = target_revision(path)?;
    require_unchanged(current.as_ref(), expected)
}

fn require_unchanged(
    current: Option<&Revision>,
    expected: Option<&Revision>,
) -> Result<(), StoreError> {
    if current == expected {
        Ok(())
    } else {
        Err(StoreError::StaleTargetRevision)
    }
}

pub(super) fn metadata_revision(
    path: &Path,
    metadata: &fs::Metadata,
) -> Result<Revision, StoreError> {
    Ok(revision_from_stamp(&platform::stamp(path, metadata)?))
}

pub(super) fn filesystem_identity(path: &Path) -> Result<String, StoreError> {
    let stamp = platform::stamp(path, &fs::symlink_metadata(path)?)?;
    Ok(format!("{}:{}", stamp.identity_a, hex(&stamp.identity_b)))
}

pub(super) fn open_file_revision(
    file: &fs::File,
    metadata: &fs::Metadata,
) -> Result<Revision, StoreError> {
    Ok(revision_from_stamp(&platform::open_stamp(file, metadata)?))
}

pub(super) fn store_revision<'a>(
    entries: impl IntoIterator<Item = (&'a str, &'a Revision)>,
    synthetic: bool,
) -> Revision {
    let mut hasher = Sha256::new();
    hasher.update(if synthetic {
        SYNTHETIC_REVISION_VERSION
    } else {
        STORE_REVISION_VERSION
    });
    for (path, revision) in entries {
        hash_field(&mut hasher, path.as_bytes());
        hash_field(&mut hasher, revision.0.as_bytes());
    }
    Revision(format!(
        "{}:{}",
        if synthetic {
            "synthetic-fsmeta-tree-v1"
        } else {
            "fsmeta-tree-v1"
        },
        hex(&hasher.finalize())
    ))
}

pub(super) fn synthetic_revision(path: &str, exists: bool) -> Revision {
    let mut hasher = Sha256::new();
    hasher.update(SYNTHETIC_REVISION_VERSION);
    hash_field(&mut hasher, path.as_bytes());
    hasher.update([u8::from(exists)]);
    Revision(format!("synthetic-fsmeta-v1:{}", hex(&hasher.finalize())))
}

fn revision_from_stamp(stamp: &MetadataStamp) -> Revision {
    let mut hasher = Sha256::new();
    hasher.update(METADATA_REVISION_VERSION);
    hasher.update([stamp.kind]);
    hasher.update(stamp.identity_a.to_le_bytes());
    hasher.update(stamp.identity_b);
    hasher.update(stamp.length.to_le_bytes());
    hasher.update(stamp.modified_seconds.to_le_bytes());
    hasher.update(stamp.modified_subseconds.to_le_bytes());
    hasher.update(stamp.changed_seconds.to_le_bytes());
    hasher.update(stamp.changed_subseconds.to_le_bytes());
    Revision(format!("fsmeta-v1:{}", hex(&hasher.finalize())))
}

fn hash_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value);
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MetadataStamp {
    kind: u8,
    identity_a: u64,
    identity_b: [u8; 16],
    length: u64,
    modified_seconds: i64,
    modified_subseconds: i64,
    changed_seconds: i64,
    changed_subseconds: i64,
}

fn file_kind(metadata: &fs::Metadata) -> u8 {
    let kind = metadata.file_type();
    if kind.is_file() {
        1
    } else if kind.is_dir() {
        2
    } else if kind.is_symlink() {
        3
    } else {
        4
    }
}

#[cfg(unix)]
mod platform {
    use super::{MetadataStamp, StoreError, file_kind};
    use std::{fs, os::unix::fs::MetadataExt, path::Path};

    pub(super) fn stamp(
        _path: &Path,
        metadata: &fs::Metadata,
    ) -> Result<MetadataStamp, StoreError> {
        Ok(from_metadata(metadata))
    }

    pub(super) fn open_stamp(
        _file: &fs::File,
        metadata: &fs::Metadata,
    ) -> Result<MetadataStamp, StoreError> {
        Ok(from_metadata(metadata))
    }

    fn from_metadata(metadata: &fs::Metadata) -> MetadataStamp {
        let mut identity_b = [0; 16];
        identity_b[..8].copy_from_slice(&metadata.ino().to_le_bytes());
        MetadataStamp {
            kind: file_kind(metadata),
            identity_a: metadata.dev(),
            identity_b,
            length: metadata.len(),
            modified_seconds: metadata.mtime(),
            modified_subseconds: metadata.mtime_nsec(),
            changed_seconds: metadata.ctime(),
            changed_subseconds: metadata.ctime_nsec(),
        }
    }
}

#[cfg(windows)]
mod platform {
    use super::{MetadataStamp, StoreError, file_kind};
    use std::{
        fs::{self, OpenOptions},
        mem::size_of,
        os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
        path::Path,
    };
    use windows_sys::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{
            FILE_BASIC_INFO, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
            FILE_ID_INFO, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FileBasicInfo,
            FileIdInfo, GetFileInformationByHandleEx,
        },
    };

    pub(super) fn stamp(path: &Path, metadata: &fs::Metadata) -> Result<MetadataStamp, StoreError> {
        let file = OpenOptions::new()
            .access_mode(0)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?;
        open_stamp(&file, metadata)
    }

    pub(super) fn open_stamp(
        file: &fs::File,
        metadata: &fs::Metadata,
    ) -> Result<MetadataStamp, StoreError> {
        let handle = file.as_raw_handle() as HANDLE;
        let mut basic = FILE_BASIC_INFO::default();
        let mut identity = FILE_ID_INFO::default();
        let basic_ok = unsafe {
            GetFileInformationByHandleEx(
                handle,
                FileBasicInfo,
                std::ptr::addr_of_mut!(basic).cast(),
                size_of::<FILE_BASIC_INFO>() as u32,
            )
        };
        if basic_ok == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let identity_ok = unsafe {
            GetFileInformationByHandleEx(
                handle,
                FileIdInfo,
                std::ptr::addr_of_mut!(identity).cast(),
                size_of::<FILE_ID_INFO>() as u32,
            )
        };
        if identity_ok == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(MetadataStamp {
            kind: file_kind(metadata),
            identity_a: identity.VolumeSerialNumber,
            identity_b: identity.FileId.Identifier,
            length: metadata.len(),
            modified_seconds: basic.LastWriteTime,
            modified_subseconds: 0,
            changed_seconds: basic.ChangeTime,
            changed_subseconds: 0,
        })
    }
}

#[cfg(not(any(unix, windows)))]
compile_error!("filesystem metadata revisions require a Unix or Windows target");

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, io::Write};
    use tempfile::TempDir;

    #[test]
    fn metadata_revision_detects_ordinary_edits_replacements_types_and_existence() {
        let root = TempDir::new().expect("root");
        let path = root.path().join("target");
        assert_eq!(target_revision(&path).expect("absent"), None);

        fs::write(&path, b"one").expect("create");
        let created = target_revision(&path).expect("created").expect("revision");
        let mut file = fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&path)
            .expect("open edit");
        file.write_all(b"longer").expect("edit");
        file.sync_all().expect("sync edit");
        let edited = target_revision(&path).expect("edited").expect("revision");
        assert_ne!(created, edited);

        let replacement = root.path().join("replacement");
        fs::write(&replacement, b"longer").expect("replacement");
        fs::rename(&replacement, &path).expect("atomic replace");
        let replaced = target_revision(&path).expect("replaced").expect("revision");
        assert_ne!(edited, replaced);

        fs::remove_file(&path).expect("remove file");
        fs::create_dir(&path).expect("replace with directory");
        let directory = target_revision(&path)
            .expect("directory")
            .expect("revision");
        assert_ne!(replaced, directory);
        fs::remove_dir(&path).expect("remove directory");
        assert_eq!(target_revision(&path).expect("deleted"), None);
    }

    #[test]
    fn controlled_same_identity_size_and_timestamps_edit_is_admitted() {
        let stamp = MetadataStamp {
            kind: 1,
            identity_a: 7,
            identity_b: [9; 16],
            length: 4,
            modified_seconds: 11,
            modified_subseconds: 12,
            changed_seconds: 13,
            changed_subseconds: 14,
        };
        let before_bytes = b"left";
        let after_bytes = b"rite";
        assert_ne!(before_bytes, after_bytes);
        let revision = revision_from_stamp(&stamp);
        require_unchanged(Some(&revision), Some(&revision)).expect("metadata-equivalent apply");
    }

    #[test]
    fn revision_covers_type_native_identity_size_and_both_timestamps() {
        let base = MetadataStamp {
            kind: 1,
            identity_a: 2,
            identity_b: [3; 16],
            length: 4,
            modified_seconds: 5,
            modified_subseconds: 6,
            changed_seconds: 7,
            changed_subseconds: 8,
        };
        let baseline = revision_from_stamp(&base);
        for changed in [
            MetadataStamp {
                kind: 9,
                ..base.clone()
            },
            MetadataStamp {
                identity_a: 9,
                ..base.clone()
            },
            MetadataStamp {
                identity_b: [9; 16],
                ..base.clone()
            },
            MetadataStamp {
                length: 9,
                ..base.clone()
            },
            MetadataStamp {
                modified_seconds: 9,
                ..base.clone()
            },
            MetadataStamp {
                modified_subseconds: 9,
                ..base.clone()
            },
            MetadataStamp {
                changed_seconds: 9,
                ..base.clone()
            },
            MetadataStamp {
                changed_subseconds: 9,
                ..base.clone()
            },
        ] {
            assert_ne!(revision_from_stamp(&changed), baseline);
        }
    }
}
