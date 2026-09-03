use std::{
    fs,
    path::{Path, PathBuf},
};

use super::{InstalledRuntime, RuntimeError, RuntimePaths};

#[derive(Clone, Copy)]
pub(super) enum LinkKind {
    File,
    Directory,
}

pub(super) struct ActivationLink {
    pub target: PathBuf,
    pub source: PathBuf,
    pub kind: LinkKind,
}

impl ActivationLink {
    pub(super) fn file(target: PathBuf, source: PathBuf) -> Self {
        Self {
            target,
            source,
            kind: LinkKind::File,
        }
    }

    pub(super) fn directory(target: PathBuf, source: PathBuf) -> Self {
        Self {
            target,
            source,
            kind: LinkKind::Directory,
        }
    }
}

pub(super) struct PreparedActivation {
    config_target: PathBuf,
    config_staged: PathBuf,
    config_backup: Option<PathBuf>,
    config_published: bool,
    links: Vec<PreparedLink>,
    committed: bool,
}

struct PreparedLink {
    target: PathBuf,
    staged: PathBuf,
    created: bool,
    active: bool,
    kind: LinkKind,
    backup: Option<PathBuf>,
}

impl PreparedActivation {
    pub(super) fn recover_config(paths: &RuntimePaths) -> Result<(), RuntimeError> {
        let target = paths.config_file();
        let backup = hidden_sibling(&target, "previous")?;
        if !path_present(&target)? && path_present(&backup)? {
            fs::rename(&backup, &target)?;
            sync_parent(&target)?;
        }
        Ok(())
    }

    pub(super) fn has_config_backup(paths: &RuntimePaths) -> Result<bool, RuntimeError> {
        path_present(&hidden_sibling(&paths.config_file(), "previous")?)
    }

    pub(super) fn restore_config_backup(paths: &RuntimePaths) -> Result<(), RuntimeError> {
        let target = paths.config_file();
        let backup = hidden_sibling(&target, "previous")?;
        remove_if_present(&target)?;
        fs::rename(backup, &target)?;
        sync_parent(&target)
    }

    pub(super) fn active_links_match(links: &[ActivationLink]) -> Result<bool, RuntimeError> {
        for link in links {
            if !link_points_to(&link.target, &link.source)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(super) fn recover_active(
        paths: &RuntimePaths,
        links: Vec<ActivationLink>,
    ) -> Result<(), RuntimeError> {
        for link in links {
            let staged = hidden_sibling(&link.target, "next")?;
            let backup = hidden_sibling(&link.target, "previous")?;
            remove_link_if_present(&staged, link.kind)?;
            if link_points_to(&link.target, &link.source)? {
                remove_link_if_present(&backup, link.kind)?;
            } else if link_points_to(&backup, &link.source)? {
                remove_link_if_present(&link.target, link.kind)?;
                fs::rename(&backup, &link.target)?;
                sync_parent(&link.target)?;
            }
        }
        remove_if_present(&paths.config_file().with_extension("json.next"))?;
        remove_if_present(&hidden_sibling(&paths.config_file(), "previous")?)?;
        Ok(())
    }

    pub(super) fn remove_artifacts(
        paths: &RuntimePaths,
        links: Vec<ActivationLink>,
    ) -> Result<(), RuntimeError> {
        for link in links {
            remove_link_if_present(&hidden_sibling(&link.target, "next")?, link.kind)?;
            remove_link_if_present(&hidden_sibling(&link.target, "previous")?, link.kind)?;
        }
        remove_if_present(&paths.config_file().with_extension("json.next"))?;
        remove_if_present(&hidden_sibling(&paths.config_file(), "previous")?)
    }

    pub(super) fn prepare(
        paths: &RuntimePaths,
        installed: &InstalledRuntime,
        links: Vec<ActivationLink>,
    ) -> Result<Self, RuntimeError> {
        let config_target = paths.config_file();
        let config_staged = config_target.with_extension("json.next");
        remove_if_present(&config_staged)?;
        let config_backup = match config_target.symlink_metadata() {
            Ok(_) => Some(hidden_sibling(&config_target, "previous")?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(RuntimeError::Io(error)),
        };
        if let Some(backup) = &config_backup {
            remove_if_present(backup)?;
        }

        let mut config = serde_json::to_vec_pretty(installed)?;
        config.push(b'\n');
        fs::write(&config_staged, config)?;
        fs::File::open(&config_staged)?.sync_all()?;

        let links = links
            .into_iter()
            .map(prepare_link)
            .collect::<Result<Vec<_>, RuntimeError>>()?;
        sync_parent(&config_staged)?;
        for link in &links {
            sync_parent(&link.staged)?;
        }

        Ok(Self {
            config_target,
            config_staged,
            config_backup,
            config_published: false,
            links,
            committed: false,
        })
    }

    pub(super) fn commit(mut self) -> Result<(), RuntimeError> {
        let activation = (|| {
            for link in &mut self.links {
                link.publish()?;
            }
            if let Some(backup) = &self.config_backup {
                fs::rename(&self.config_target, backup)?;
            }
            replace_staged(&self.config_staged, &self.config_target, LinkKind::File)?;
            self.config_published = true;
            sync_parent(&self.config_target)?;
            Ok(())
        })();
        if let Err(activation) = activation {
            return match self.restore_previous() {
                Ok(()) => Err(activation),
                Err(rollback) => Err(RuntimeError::ActivationRollback {
                    activation: activation.to_string(),
                    rollback: rollback.to_string(),
                }),
            };
        }
        for link in &mut self.links {
            link.active = true;
        }
        self.committed = true;
        for link in &self.links {
            let _ = link.remove_backup();
        }
        if let Some(backup) = &self.config_backup {
            let _ = remove_if_present(backup);
        }
        Ok(())
    }

    fn restore_previous(&mut self) -> Result<(), RuntimeError> {
        if self.config_published {
            remove_if_present(&self.config_target)?;
            self.config_published = false;
        }
        if let Some(backup) = &self.config_backup {
            if backup.exists() {
                fs::rename(backup, &self.config_target)?;
            }
        }
        for link in self.links.iter_mut().rev() {
            link.restore()?;
        }
        sync_parent(&self.config_target)
    }
}

impl Drop for PreparedActivation {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_file(&self.config_staged);
            let _ = self.restore_previous();
        }
    }
}

impl Drop for PreparedLink {
    fn drop(&mut self) {
        if !self.active {
            let _ = self.restore();
        }
    }
}

impl PreparedLink {
    fn publish(&mut self) -> Result<(), RuntimeError> {
        if let Some(backup) = &self.backup {
            remove_link_if_present(backup, self.kind)?;
            fs::rename(&self.target, backup)?;
        }
        replace_staged(&self.staged, &self.target, self.kind)?;
        self.created = true;
        sync_parent(&self.target)
    }

    fn restore(&mut self) -> Result<(), RuntimeError> {
        remove_link_if_present(&self.staged, self.kind)?;
        if self.created {
            remove_link_if_present(&self.target, self.kind)?;
            self.created = false;
        }
        if let Some(backup) = &self.backup {
            if backup.exists() {
                fs::rename(backup, &self.target)?;
            }
        }
        Ok(())
    }

    fn remove_backup(&self) -> Result<(), RuntimeError> {
        if let Some(backup) = &self.backup {
            remove_link_if_present(backup, self.kind)?;
        }
        Ok(())
    }
}

fn prepare_link(link: ActivationLink) -> Result<PreparedLink, RuntimeError> {
    let backup = match link.target.symlink_metadata() {
        Ok(_) => Some(hidden_sibling(&link.target, "previous")?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(RuntimeError::Io(error)),
    };
    let staged = hidden_sibling(&link.target, "next")?;
    remove_link_if_present(&staged, link.kind)?;
    create_entrypoint(&link.source, &staged, link.kind)?;
    Ok(PreparedLink {
        target: link.target,
        staged,
        created: false,
        active: false,
        kind: link.kind,
        backup,
    })
}

fn hidden_sibling(target: &Path, suffix: &str) -> Result<PathBuf, RuntimeError> {
    let parent = target.parent().ok_or_else(|| {
        RuntimeError::InvalidEnvironment(format!(
            "activation target {} has no parent directory",
            target.display()
        ))
    })?;
    let name = target.file_name().ok_or_else(|| {
        RuntimeError::InvalidEnvironment(format!(
            "activation target {} has no file name",
            target.display()
        ))
    })?;
    Ok(parent.join(format!(".{}.{suffix}", name.to_string_lossy())))
}

fn path_present(path: &Path) -> Result<bool, RuntimeError> {
    match path.symlink_metadata() {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(RuntimeError::Io(error)),
    }
}

fn link_points_to(link: &Path, target: &Path) -> Result<bool, RuntimeError> {
    match link.symlink_metadata() {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Ok(fs::read_link(link).ok().as_deref() == Some(target))
        }
        Ok(_) => Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(RuntimeError::Io(error)),
    }
}

fn remove_if_present(path: &Path) -> Result<(), RuntimeError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(RuntimeError::Io(error)),
    }
}

#[cfg(unix)]
fn create_entrypoint(source: &Path, target: &Path, _kind: LinkKind) -> Result<(), RuntimeError> {
    std::os::unix::fs::symlink(source, target)?;
    Ok(())
}

#[cfg(windows)]
fn create_entrypoint(source: &Path, target: &Path, kind: LinkKind) -> Result<(), RuntimeError> {
    match kind {
        LinkKind::File => std::os::windows::fs::symlink_file(source, target)?,
        LinkKind::Directory => std::os::windows::fs::symlink_dir(source, target)?,
    }
    Ok(())
}

#[cfg(all(not(unix), not(windows)))]
fn create_entrypoint(_source: &Path, _target: &Path, _kind: LinkKind) -> Result<(), RuntimeError> {
    Err(RuntimeError::Io(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "runtime activation links are unsupported on this platform",
    )))
}

#[cfg(unix)]
fn replace_staged(staged: &Path, target: &Path, _kind: LinkKind) -> Result<(), RuntimeError> {
    fs::rename(staged, target)?;
    Ok(())
}

#[cfg(not(unix))]
fn replace_staged(staged: &Path, target: &Path, kind: LinkKind) -> Result<(), RuntimeError> {
    remove_link_if_present(target, kind)?;
    fs::rename(staged, target)?;
    Ok(())
}

#[cfg(unix)]
fn remove_link_if_present(path: &Path, _kind: LinkKind) -> Result<(), RuntimeError> {
    remove_if_present(path)
}

#[cfg(windows)]
fn remove_link_if_present(path: &Path, kind: LinkKind) -> Result<(), RuntimeError> {
    let result = match kind {
        LinkKind::File => fs::remove_file(path),
        LinkKind::Directory => fs::remove_dir(path),
    };
    match result {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(RuntimeError::Io(error)),
    }
}

#[cfg(all(not(unix), not(windows)))]
fn remove_link_if_present(path: &Path, _kind: LinkKind) -> Result<(), RuntimeError> {
    remove_if_present(path)
}

#[cfg(unix)]
fn sync_parent(path: &Path) -> Result<(), RuntimeError> {
    if let Some(parent) = path.parent() {
        fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn sync_parent(_path: &Path) -> Result<(), RuntimeError> {
    Ok(())
}
