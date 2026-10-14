//! Fixed-owner cleanup. No paths supplied by the UI, and no reparse traversal.
use crate::gateway::Result;
use std::{fs, path::Path};

fn inspect_node(path: &Path) -> Result<fs::Metadata> {
    let metadata = fs::symlink_metadata(path).map_err(|_| "Cannot inspect managed data")?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("Managed data contains a reparse point; cleanup refused".into());
        }
    }
    if metadata.file_type().is_symlink() {
        return Err("Managed data contains a symbolic link; cleanup refused".into());
    }
    Ok(metadata)
}
fn inspect_tree(path: &Path) -> Result<()> {
    if inspect_node(path)?.is_dir() {
        for entry in fs::read_dir(path).map_err(|_| "Cannot inspect managed directory")? {
            inspect_tree(&entry.map_err(|_| "Cannot inspect managed entry")?.path())?;
        }
    }
    Ok(())
}
pub fn validate_owned(root: &Path, target: &Path) -> Result<()> {
    if !root.is_absolute()
        || !target.starts_with(root)
        || target.components().any(|c| {
            matches!(
                c,
                std::path::Component::ParentDir | std::path::Component::CurDir
            )
        })
    {
        return Err("Managed data ownership check failed".into());
    }
    // Inspect ancestors as well: a junction above the target is equally unsafe.
    for ancestor in target.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(_) => {
                inspect_node(ancestor)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("Cannot inspect managed data parent".into()),
        }
    }
    if fs::symlink_metadata(target).is_ok() {
        inspect_tree(target)?;
    }
    Ok(())
}
pub fn delete_owned(root: &Path, target: &Path) -> Result<()> {
    validate_owned(root, target)?;
    match fs::symlink_metadata(target) {
        Ok(metadata) => {
            let outcome = if metadata.is_dir() {
                fs::remove_dir_all(target)
            } else {
                fs::remove_file(target)
            };
            outcome.map_err(|_| {
                "Local data cleanup failed; registration retained for retry".to_string()
            })?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err("Cannot inspect managed data".into()),
    }
    if target.exists() {
        return Err("Local data remains after cleanup".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn isolated_cleanup_preserves_siblings_and_rejects_outside_paths() {
        let root = std::env::temp_dir().join(format!("tmsg-cleanup-{}", crate::settings::now()));
        fs::create_dir_all(root.join("app/A/db")).unwrap();
        fs::create_dir_all(root.join("app/B/db")).unwrap();
        fs::write(root.join("app/A/db/synthetic"), b"fixture").unwrap();
        assert!(delete_owned(&root.join("app"), &root.join("app/../B")).is_err());
        assert!(delete_owned(&root.join("app"), &root.join("other")).is_err());
        delete_owned(&root.join("app"), &root.join("app/A")).unwrap();
        assert!(root.join("app/B/db").exists());
        delete_owned(&root, &root.join("app")).unwrap();
        fs::remove_dir(root).unwrap();
    }
    #[test]
    fn complete_local_reset_deletes_all_categories_only_inside_synthetic_app_root() {
        let owner = std::env::temp_dir().join(format!(
            "tmsg-reset-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let root = owner.join("com.funare.tmsgpicker");
        for directory in [
            "tdlib/db",
            "tdlib/files",
            "tdlib/accounts/p-a/db",
            "tdlib/accounts/p-a/files",
            "cache",
        ] {
            fs::create_dir_all(root.join(directory)).unwrap();
            fs::write(root.join(directory).join("synthetic"), b"fixture").unwrap();
        }
        for file in [
            "app-config.json",
            "accounts.json",
            "credential-store.json",
            "settings.json",
        ] {
            fs::write(root.join(file), b"synthetic state").unwrap();
        }
        fs::write(owner.join("unrelated"), b"preserve").unwrap();
        delete_owned(&root, &root).unwrap();
        assert!(!root.exists());
        assert_eq!(fs::read(owner.join("unrelated")).unwrap(), b"preserve");
        fs::remove_file(owner.join("unrelated")).unwrap();
        fs::remove_dir(owner).unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn junction_inside_or_above_target_refuses_all_deletion() {
        let root = std::env::temp_dir().join(format!("tmsg-junction-{}", crate::settings::now()));
        fs::create_dir_all(root.join("app")).unwrap();
        fs::create_dir_all(root.join("outside")).unwrap();
        let status = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(root.join("app").join("link"))
            .arg(root.join("outside"))
            .output()
            .unwrap();
        assert!(
            status.status.success(),
            "{} {}",
            String::from_utf8_lossy(&status.stdout),
            String::from_utf8_lossy(&status.stderr)
        );
        assert!(delete_owned(&root, &root.join("app")).is_err());
        assert!(delete_owned(&root, &root.join("app/link/absent")).is_err());
        assert!(root.join("outside").exists());
        fs::remove_dir(root.join("app/link")).unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}
