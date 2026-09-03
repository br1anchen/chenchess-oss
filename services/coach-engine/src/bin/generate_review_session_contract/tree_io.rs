use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use super::PACKAGE_RELATIVE_DIR;

pub(super) fn publish_generated_tree(root: &Path, staging: &Path) -> anyhow::Result<()> {
    let target = root.join(PACKAGE_RELATIVE_DIR);
    if target.exists() {
        fs::remove_dir_all(&target)?;
    }
    copy_tree(&staging.join(PACKAGE_RELATIVE_DIR), &target)?;
    Ok(())
}

pub(super) fn verify_generated_tree(root: &Path, staging: &Path) -> anyhow::Result<()> {
    let mut drift = Vec::new();
    compare_tree(
        &root.join(PACKAGE_RELATIVE_DIR),
        &staging.join(PACKAGE_RELATIVE_DIR),
        &mut drift,
    )?;
    if drift.is_empty() {
        Ok(())
    } else {
        anyhow::bail!(
            "generated Review Session contract drifted:\n{}\nrun the generator and review the diff",
            drift.join("\n")
        )
    }
}

fn compare_tree(actual: &Path, expected: &Path, drift: &mut Vec<String>) -> anyhow::Result<()> {
    let actual_files = relative_files(actual)?;
    let expected_files = relative_files(expected)?;
    for relative in actual_files.union(&expected_files) {
        let actual_path = actual.join(relative);
        let expected_path = expected.join(relative);
        match (fs::read(&actual_path), fs::read(&expected_path)) {
            (Ok(actual_bytes), Ok(expected_bytes)) if actual_bytes == expected_bytes => {}
            (Ok(_), Ok(_)) => drift.push(format!("changed {}", actual_path.display())),
            (Ok(_), Err(_)) => drift.push(format!("unexpected {}", actual_path.display())),
            (Err(_), Ok(_)) => drift.push(format!("missing {}", actual_path.display())),
            (Err(actual_error), Err(_)) => return Err(actual_error.into()),
        }
    }
    Ok(())
}

fn relative_files(root: &Path) -> anyhow::Result<BTreeSet<PathBuf>> {
    let mut files = BTreeSet::new();
    if !root.exists() {
        return Ok(files);
    }
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            if path.is_dir() {
                if path
                    .file_name()
                    .is_some_and(|name| name == "node_modules" || name == ".turbo")
                {
                    continue;
                }
                pending.push(path);
            } else {
                files.insert(path.strip_prefix(root)?.to_path_buf());
            }
        }
    }
    Ok(files)
}

fn copy_tree(source: &Path, target: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(target)?;
    for relative in relative_files(source)? {
        let destination = target.join(&relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(source.join(relative), destination)?;
    }
    Ok(())
}
