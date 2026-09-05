use crate::{revision::filesystem_identity, store::StoreError};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    process::Command,
};

pub(super) fn acquire(root: &Path, keys: &BTreeMap<String, bool>) -> Result<Vec<File>, StoreError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "casefile-locks",
        ])
        .output()?;
    if !output.status.success() {
        return Err(StoreError::Invalid(
            "mutation coordination requires a Git worktree".into(),
        ));
    }
    let directory = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    fs::create_dir_all(&directory)?;
    let root = fs::canonicalize(root)?;
    let root_identity = filesystem_identity(&root)?;
    let mut canonical = BTreeMap::new();
    for (key, exclusive) in keys {
        let key = if let Some(path) = key.strip_prefix("path:") {
            format!("path:{}", canonical_relative(&root, path)?)
        } else {
            key.clone()
        };
        if *exclusive && canonical.get(&key) == Some(&true) {
            return Err(StoreError::Invalid(
                "mutation targets alias the same canonical file".into(),
            ));
        }
        canonical
            .entry(key)
            .and_modify(|value| *value |= exclusive)
            .or_insert(*exclusive);
    }
    canonical
        .iter()
        .map(|(key, exclusive)| {
            let key = format!("{root_identity}\0{key}");
            // Stable sidecars must not be unlinked: target inodes change on replacement.
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(directory.join(format!("{:x}", Sha256::digest(key.as_bytes()))))?;
            if *exclusive {
                file.lock()?;
            } else {
                file.lock_shared()?;
            }
            Ok(file)
        })
        .collect()
}

fn canonical_relative(root: &Path, path: &str) -> Result<String, StoreError> {
    let mut parent = root.to_path_buf();
    let mut sensitive = case_sensitive(&parent)?;
    let mut names = Vec::new();
    for name in path.split('/') {
        if parent.is_dir() {
            sensitive = case_sensitive(&parent)?;
        }
        names.push(if sensitive {
            name.to_owned()
        } else {
            name.to_lowercase()
        });
        parent.push(name);
    }
    Ok(names.join("/"))
}

#[cfg(target_os = "macos")]
fn case_sensitive(path: &Path) -> Result<bool, StoreError> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let path = CString::new(path.as_os_str().as_bytes())
        .map_err(|error| StoreError::Invalid(error.to_string()))?;
    let result = unsafe { libc::pathconf(path.as_ptr(), libc::_PC_CASE_SENSITIVE) };
    if result < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(result != 0)
}

#[cfg(windows)]
fn case_sensitive(path: &Path) -> Result<bool, StoreError> {
    use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
    use windows_sys::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{
            FILE_CASE_SENSITIVE_INFO, FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE,
            FILE_SHARE_READ, FILE_SHARE_WRITE, FileCaseSensitiveInfo, GetFileInformationByHandleEx,
        },
        System::SystemServices::FILE_CS_FLAG_CASE_SENSITIVE_DIR,
    };
    let directory = OpenOptions::new()
        .access_mode(0)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
    let mut info = FILE_CASE_SENSITIVE_INFO::default();
    let result = unsafe {
        GetFileInformationByHandleEx(
            directory.as_raw_handle() as HANDLE,
            FileCaseSensitiveInfo,
            std::ptr::addr_of_mut!(info).cast(),
            size_of::<FILE_CASE_SENSITIVE_INFO>() as u32,
        )
    };
    if result == 0 {
        let error = std::io::Error::last_os_error();
        use windows_sys::Win32::Foundation::{ERROR_INVALID_PARAMETER, ERROR_NOT_SUPPORTED};
        if matches!(error.raw_os_error(), Some(code) if code == ERROR_INVALID_PARAMETER as i32 || code == ERROR_NOT_SUPPORTED as i32)
        {
            return Ok(false);
        }
        return Err(error.into());
    }
    Ok(info.Flags & FILE_CS_FLAG_CASE_SENSITIVE_DIR != 0)
}

#[cfg(not(any(windows, target_os = "macos")))]
fn case_sensitive(path: &Path) -> Result<bool, StoreError> {
    // Consult an existing child rather than infer sensitivity from the host OS: mounted
    // filesystems can accept case aliases even on Unix.
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let alternate = name
            .chars()
            .map(|c| {
                if c.is_ascii_lowercase() {
                    c.to_ascii_uppercase()
                } else {
                    c.to_ascii_lowercase()
                }
            })
            .collect::<String>();
        if alternate == name {
            continue;
        }
        let alias = path.join(alternate);
        match fs::symlink_metadata(&alias) {
            Ok(metadata) if !metadata.file_type().is_symlink() => {
                return Ok(filesystem_identity(&entry.path())? != filesystem_identity(&alias)?);
            }
            Ok(_) => continue,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
            Err(error) => return Err(error.into()),
        }
    }
    match path.parent() {
        Some(parent) if parent != path => case_sensitive(parent),
        _ => Ok(true),
    }
}

pub(super) fn canonical_target(root: &Path, relative: &str) -> Result<String, StoreError> {
    let target = root.join(relative);
    let metadata = match fs::symlink_metadata(&target) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(relative.into()),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_file() {
        return Ok(relative.into());
    }
    let path = Path::new(relative);
    let parent = path.parent().unwrap_or(Path::new(""));
    crate::store::require_safe_target_parent(root, parent, "mutation target")?;
    let name = path.file_name().expect("checked target filename");
    let entries = fs::read_dir(root.join(parent))?.collect::<Result<Vec<_>, _>>()?;
    if entries.iter().any(|entry| entry.file_name() == name) {
        return Ok(relative.into());
    }
    let identity = filesystem_identity(&target)?;
    let matches = entries
        .into_iter()
        .filter_map(|entry| {
            let matching = entry.file_name().to_string_lossy().to_lowercase()
                == name.to_string_lossy().to_lowercase();
            matching.then_some(entry)
        })
        .filter_map(|entry| match filesystem_identity(&entry.path()) {
            Ok(value) if value == identity => Some(Ok(entry.file_name())),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<Result<Vec<_>, StoreError>>()?;
    if let [name] = matches.as_slice() {
        let parent = parent.to_string_lossy().replace('\\', "/");
        return Ok(if parent.is_empty() {
            name.to_string_lossy().into_owned()
        } else {
            format!("{parent}/{}", name.to_string_lossy())
        });
    }
    // Native canonicalization handles aliases outside the portable case-folded filename form.
    let resolved = fs::canonicalize(target)?;
    let root = fs::canonicalize(root)?;
    let relative = resolved
        .strip_prefix(root)
        .map_err(|_| StoreError::Invalid("mutation target escaped the Store root".into()))?;
    Ok(relative.to_string_lossy().replace('\\', "/"))
}
