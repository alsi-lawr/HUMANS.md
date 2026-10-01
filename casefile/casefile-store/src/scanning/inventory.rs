use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InventoryKind {
    Regular,
    Symlink,
    Other,
}

pub(crate) struct InventoryEntry {
    pub(crate) path: PathBuf,
    pub(crate) kind: InventoryKind,
    pub(crate) revision: Revision,
}

pub(crate) struct MetadataInventory {
    pub(crate) entries: BTreeMap<String, InventoryEntry>,
    pub(crate) revision: Revision,
    directories: BTreeMap<PathBuf, Vec<std::ffi::OsString>>,
}

pub(crate) fn metadata_inventory(root: &Path) -> Result<MetadataInventory, StoreError> {
    let mut entries = BTreeMap::new();
    let mut directories = BTreeMap::from([(root.to_owned(), Vec::new())]);
    collect_walk(
        root,
        root,
        &mut |path, entry| {
            entries.insert(path, entry);
        },
        |_, _| true,
        Some(&mut directories),
    )?;
    for names in directories.values_mut() {
        names.sort_unstable();
    }
    let revision = store_revision(
        entries
            .iter()
            .map(|(path, entry)| (path.as_str(), &entry.revision)),
        false,
    );
    Ok(MetadataInventory {
        entries,
        revision,
        directories,
    })
}

pub(crate) fn collect_selected(
    root: &Path,
    directory: &Path,
    entries: &mut Vec<(String, InventoryEntry)>,
    include: impl Fn(&Path, bool) -> bool,
) -> Result<(), StoreError> {
    collect_walk(
        root,
        directory,
        &mut |path, entry| entries.push((path, entry)),
        include,
        None,
    )
}

fn collect_walk(
    root: &Path,
    directory: &Path,
    entries: &mut impl FnMut(String, InventoryEntry),
    include: impl Fn(&Path, bool) -> bool,
    mut directories: Option<&mut BTreeMap<PathBuf, Vec<std::ffi::OsString>>>,
) -> Result<(), StoreError> {
    let relative_directory = directory
        .strip_prefix(root)
        .map_err(|_| StoreError::Invalid("traversal escaped root".into()))?;
    crate::store::require_safe_target_parent(root, relative_directory, "Store traversal")?;
    match fs::symlink_metadata(directory) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
        Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => {
            return Err(StoreError::Invalid(
                "Store traversal target must be a non-symlink directory".into(),
            ));
        }
        Ok(_) => {}
    }
    for item in walkdir::WalkDir::new(directory)
        .min_depth(1)
        .follow_links(false)
        .follow_root_links(false)
        .into_iter()
        .filter_entry(|entry| {
            let relative = entry
                .path()
                .strip_prefix(root)
                .expect("walk stays below root");
            !is_store_path_excluded(relative) && include(relative, entry.file_type().is_dir())
        })
    {
        let item = item.map_err(|error| {
            StoreError::Io(
                error
                    .into_io_error()
                    .unwrap_or_else(|| std::io::Error::other("Store traversal failed")),
            )
        })?;
        if let Some(directories) = &mut directories {
            directories
                .entry(item.path().parent().expect("walk entry parent").to_owned())
                .or_default()
                .push(item.file_name().to_owned());
            if item.file_type().is_dir() {
                directories.entry(item.path().to_owned()).or_default();
            }
        }
        if item.file_type().is_dir() {
            continue;
        }
        let path = item.into_path();
        let relative = relative(root, &path)?;
        let metadata = fs::symlink_metadata(&path)?;
        let kind = if metadata.file_type().is_symlink() {
            InventoryKind::Symlink
        } else if metadata.is_file() {
            InventoryKind::Regular
        } else {
            InventoryKind::Other
        };
        let revision = metadata_revision(&path, &metadata)?;
        entries(
            relative,
            InventoryEntry {
                path,
                kind,
                revision,
            },
        );
    }
    Ok(())
}

pub(crate) fn inventory_target(
    root: &Path,
    relative: &str,
) -> Result<Option<InventoryEntry>, StoreError> {
    let path = root.join(relative);
    crate::store::require_safe_target_parent(
        root,
        Path::new(relative)
            .parent()
            .unwrap_or_else(|| Path::new("")),
        relative,
    )?;
    match fs::symlink_metadata(&path) {
        Ok(metadata) => {
            let kind = if metadata.file_type().is_symlink() {
                InventoryKind::Symlink
            } else if metadata.is_file() {
                InventoryKind::Regular
            } else {
                InventoryKind::Other
            };
            Ok(Some(InventoryEntry {
                revision: metadata_revision(&path, &metadata)?,
                path,
                kind,
            }))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn read_inventory_entry(entry: &InventoryEntry) -> Result<Vec<u8>, StoreError> {
    read_entry(entry, true)
}

pub(crate) fn read_observed_entry(
    root: &Path,
    relative: &str,
    entry: &InventoryEntry,
) -> Result<Vec<u8>, StoreError> {
    crate::store::require_safe_target_parent(
        root,
        Path::new(relative)
            .parent()
            .unwrap_or_else(|| Path::new("")),
        relative,
    )?;
    read_entry(entry, false)
}

fn read_entry(entry: &InventoryEntry, verify_path: bool) -> Result<Vec<u8>, StoreError> {
    #[cfg(test)]
    crate::checking::observe_open(&entry.path);
    let mut file = File::open(&entry.path)?;
    let opened_metadata = file.metadata()?;
    if !opened_metadata.is_file() {
        return Err(StoreError::Invalid(
            "Store content changed before it was opened".into(),
        ));
    }
    let opened_revision = open_file_revision(&file, &opened_metadata)?;
    if opened_revision != entry.revision {
        return Err(StoreError::Invalid(
            "Store content changed before it was read".into(),
        ));
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let after_open_revision = if verify_path {
        open_file_revision(&file, &file.metadata()?)?
    } else {
        opened_revision.clone()
    };
    let path_changed = if verify_path {
        let after_path_metadata = fs::symlink_metadata(&entry.path)?;
        opened_revision != metadata_revision(&entry.path, &after_path_metadata)?
    } else {
        false
    };
    if opened_revision != after_open_revision || path_changed {
        return Err(StoreError::Invalid(
            "Store content changed while it was read".into(),
        ));
    }
    Ok(bytes)
}

pub(crate) fn require_inventory_unchanged(
    root: &Path,
    baseline: &MetadataInventory,
) -> Result<(), StoreError> {
    for (path, entry) in &baseline.entries {
        let current = inventory_target(root, path)?;
        if current.as_ref().map(|entry| &entry.revision) != Some(&entry.revision) {
            return Err(StoreError::Invalid(
                "Store contents changed during read".into(),
            ));
        }
    }
    for (directory, names) in &baseline.directories {
        let relative = directory.strip_prefix(root).expect("contained directory");
        crate::store::require_safe_target_parent(root, relative, "Store membership observation")?;
        let metadata = fs::symlink_metadata(directory)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(StoreError::Invalid(
                "Store directory changed during read".into(),
            ));
        }
        let mut current = Vec::new();
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if !is_store_path_excluded(
                entry
                    .path()
                    .strip_prefix(root)
                    .expect("contained directory"),
            ) {
                current.push(entry.file_name());
            }
        }
        current.sort_unstable();
        if &current != names {
            return Err(StoreError::Invalid(
                "Store membership changed during read".into(),
            ));
        }
    }
    Ok(())
}

fn relative(root: &Path, path: &Path) -> Result<String, StoreError> {
    let path = path
        .strip_prefix(root)
        .map_err(|_| StoreError::Invalid("path escaped root".into()))?;
    let mut relative = String::new();
    for component in path.components() {
        let value = component.as_os_str().to_str().ok_or_else(|| {
            StoreError::Invalid(format!(
                "Store path cannot be represented as UTF-8: {path:?}"
            ))
        })?;
        if !relative.is_empty() {
            relative.push('/');
        }
        relative.push_str(value);
    }
    Ok(relative)
}
