use std::fs;

use serde::{Deserialize, Serialize};

use super::{
    docker::{maia_container, model_volume},
    valid_unit_version, RuntimeError, RuntimeManifest, RuntimePaths,
};

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct InstalledRuntime {
    pub(super) manifest: RuntimeManifest,
    pub(super) cli_sha256: String,
    pub(super) skill_sha256: String,
    pub(super) stockfish_binary_sha256: String,
    pub(super) owner_id: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PendingInstall {
    unit_version: String,
    owner_id: String,
}

impl InstalledRuntime {
    pub(super) fn model_volume(&self) -> String {
        model_volume(&self.manifest.unit_version, &self.owner_id)
    }

    pub(super) fn container_name(&self) -> String {
        maia_container(&self.manifest.unit_version, &self.owner_id)
    }
}

pub(super) fn validate_installed_identity(
    installed: &InstalledRuntime,
) -> Result<(), RuntimeError> {
    if !valid_unit_version(&installed.manifest.unit_version)
        || uuid::Uuid::parse_str(&installed.owner_id).is_err()
    {
        return Err(RuntimeError::InvalidManifest(
            "installed runtime identity is invalid".to_string(),
        ));
    }
    Ok(())
}

pub(super) fn read_installed(paths: &RuntimePaths) -> Result<InstalledRuntime, RuntimeError> {
    Ok(serde_json::from_slice(&fs::read(paths.config_file())?)?)
}

pub(super) fn read_installed_if_present(
    paths: &RuntimePaths,
) -> Result<Option<InstalledRuntime>, RuntimeError> {
    match fs::read(paths.config_file()) {
        Ok(config) => Ok(Some(serde_json::from_slice(&config)?)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(RuntimeError::Io(error)),
    }
}

pub(super) fn pending_install_owner(
    paths: &RuntimePaths,
    unit_version: &str,
) -> Result<String, RuntimeError> {
    let pending_path = paths.pending_install_file();
    match fs::read(&pending_path) {
        Ok(contents) => {
            let pending: PendingInstall = serde_json::from_slice(&contents)?;
            if pending.unit_version != unit_version
                || uuid::Uuid::parse_str(&pending.owner_id).is_err()
            {
                return Err(RuntimeError::InvalidEnvironment(format!(
                    "pending installation state at {} does not match runtime {unit_version}",
                    pending_path.display()
                )));
            }
            Ok(pending.owner_id)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let pending = PendingInstall {
                unit_version: unit_version.to_string(),
                owner_id: uuid::Uuid::new_v4().to_string(),
            };
            let mut contents = serde_json::to_vec_pretty(&pending)?;
            contents.push(b'\n');
            fs::write(&pending_path, contents)?;
            Ok(pending.owner_id)
        }
        Err(error) => Err(RuntimeError::Io(error)),
    }
}

pub(super) fn clear_pending_install(paths: &RuntimePaths) -> Result<(), RuntimeError> {
    match fs::remove_file(paths.pending_install_file()) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(RuntimeError::Io(error)),
    }
}
