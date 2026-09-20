use super::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ReaderMetadata {
    pub(super) public: PresentationFileMetadata,
}

pub(super) trait PresentationReader: Send + Sync {
    fn activation(&self) -> Result<(ActivationState, Activation, Vec<Diagnostic>), StoreError>;
    fn read_dir(&self, relative: &str) -> Result<Vec<String>, StoreError>;
    fn metadata(&self, relative: &str) -> Result<ReaderMetadata, StoreError>;
    fn read(&self, relative: &str) -> Result<Vec<u8>, StoreError>;
}

pub(super) struct FsPresentationReader {
    pub(super) root: PathBuf,
}

impl FsPresentationReader {
    fn validate_root(&self) -> Result<(), StoreError> {
        let metadata = fs::symlink_metadata(&self.root)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(StoreError::Invalid(
                "presentation root must remain a non-symlink directory".into(),
            ));
        }
        Ok(())
    }

    fn target(&self, relative: &str) -> Result<PathBuf, StoreError> {
        if relative.is_empty() {
            return Ok(self.root.clone());
        }
        let canonical = normalize_planning_relative(relative)
            .map_err(|message| StoreError::Invalid(message.into()))?;
        if canonical != relative || is_store_path_excluded(Path::new(relative)) {
            return Err(StoreError::Invalid(
                "presentation path must be canonical, contained, and included".into(),
            ));
        }
        Ok(self.root.join(relative))
    }

    fn validate_ancestors(&self, relative: &str) -> Result<(), StoreError> {
        self.validate_root()?;
        let relative = Path::new(relative);
        let mut current = self.root.clone();
        if let Some(parent) = relative.parent() {
            for component in parent.components() {
                current.push(component);
                let metadata = fs::symlink_metadata(&current)?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(StoreError::Invalid(
                        "presentation path ancestors must remain contained non-symlink directories"
                            .into(),
                    ));
                }
            }
        }
        Ok(())
    }
}

impl PresentationReader for FsPresentationReader {
    fn activation(&self) -> Result<(ActivationState, Activation, Vec<Diagnostic>), StoreError> {
        self.validate_root()?;
        activation(&self.root)
    }

    fn read_dir(&self, relative: &str) -> Result<Vec<String>, StoreError> {
        let directory = self.target(relative)?;
        let metadata = match fs::symlink_metadata(&directory) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        self.validate_ancestors(relative)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(StoreError::Invalid(
                "presentation catalogue directories must remain contained non-symlink directories"
                    .into(),
            ));
        }
        let mut entries = Vec::new();
        match fs::read_dir(directory) {
            Ok(values) => {
                for value in values {
                    let value = value?;
                    let name = value.file_name().to_string_lossy().into_owned();
                    entries.push(if relative.is_empty() {
                        name
                    } else {
                        format!("{relative}/{name}")
                    });
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        }
        entries.sort();
        Ok(entries)
    }

    fn metadata(&self, relative: &str) -> Result<ReaderMetadata, StoreError> {
        let target = self.root.join(relative);
        let metadata = fs::symlink_metadata(&target)?;
        let kind = if metadata.file_type().is_symlink() {
            PresentationFileKind::Symlink
        } else if metadata.is_dir() {
            PresentationFileKind::Directory
        } else if metadata.is_file() {
            PresentationFileKind::Regular
        } else {
            PresentationFileKind::Other
        };
        let modified_unix_nanos = metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
            .map(|value| value.as_nanos());
        let length = metadata.len();
        let revision = display_revision(&metadata);
        Ok(ReaderMetadata {
            public: PresentationFileMetadata {
                kind,
                length,
                modified_unix_nanos,
                revision,
            },
        })
    }

    fn read(&self, relative: &str) -> Result<Vec<u8>, StoreError> {
        self.validate_ancestors(relative)?;
        let target = self.target(relative)?;
        let metadata = fs::symlink_metadata(&target)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(StoreError::Invalid(
                "presentation content must remain a regular non-symlink file".into(),
            ));
        }
        let mut options = fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
            options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
        }
        let mut file = options.open(&target)?;
        let opened_metadata = file.metadata()?;
        if !opened_metadata.is_file() {
            return Err(StoreError::Invalid(
                "presentation content must remain a regular file".into(),
            ));
        }
        let mut bytes = Vec::with_capacity(opened_metadata.len() as usize);
        file.read_to_end(&mut bytes)?;
        Ok(bytes)
    }
}

pub(super) fn display_revision(metadata: &fs::Metadata) -> Revision {
    let modified = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_nanos());
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Revision(format!(
            "display:{}:{}:{}:{modified:?}:{}:{}",
            metadata.dev(),
            metadata.ino(),
            metadata.len(),
            metadata.ctime(),
            metadata.ctime_nsec()
        ))
    }
    #[cfg(not(unix))]
    {
        Revision(format!(
            "display:{}:{modified:?}:{:?}",
            metadata.len(),
            metadata.created().ok()
        ))
    }
}
