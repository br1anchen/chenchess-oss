use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use serde::Serialize;

use super::{
    acquire_runtime_lock, docker_inspect, owned_container_running, read_installed,
    remove_owned_container, run_checked, run_checked_owned, skill, start_container,
    validate_installed_identity, validate_manifest, verify_install_link, verify_sha256,
    verify_stockfish_uci, Platform, ProcessRunner, RuntimeError, RuntimeMutationLock, RuntimePaths,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MaiaServiceState {
    Running,
    Stopped,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaiaStatus {
    pub state: MaiaServiceState,
    pub unit_version: String,
    pub image: String,
    pub package: String,
    pub model: String,
    pub resources: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorReport {
    pub unit_version: String,
    pub skill_sha256: String,
    pub stockfish_version: String,
    pub stockfish_archive_sha256: String,
    pub stockfish_binary_sha256: String,
    pub maia_image: String,
    pub maia_package: String,
    pub maia_model: String,
    pub maia_model_sha256: String,
    pub maia_config_sha256: String,
    pub maia_port: u16,
    pub docker_version: String,
    pub maia_state: MaiaServiceState,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallOutcome {
    pub unit_version: String,
}

#[derive(Debug)]
pub struct ReviewRuntimeConfig {
    pub unit_version: String,
    pub stockfish_path: PathBuf,
    pub stockfish_version: String,
    pub maia_base_url: String,
    pub maia_image: String,
    pub maia_package: String,
    pub maia_model: String,
    pub maia_model_sha256: String,
    pub maia_config_sha256: String,
    maia_container_name: String,
}

pub struct ReviewRuntimeLease {
    pub config: ReviewRuntimeConfig,
    _lock: RuntimeMutationLock,
}

/// The installed Stockfish alone, for work that never consults the Human Move
/// Model and so must not pay for or depend on the Maia service.
#[derive(Debug)]
pub struct EngineRuntimeConfig {
    pub unit_version: String,
    pub stockfish_path: PathBuf,
    pub stockfish_version: String,
}

pub struct RuntimeManager<R> {
    runner: R,
}

#[derive(Clone, Copy)]
enum RuntimeStartupMode {
    Reuse,
    Restart,
}

impl<R: ProcessRunner> RuntimeManager<R> {
    pub fn new(runner: R) -> Self {
        Self { runner }
    }

    pub fn doctor(
        &self,
        paths: &RuntimePaths,
        platform: Platform,
    ) -> Result<DoctorReport, RuntimeError> {
        let _lock = acquire_runtime_lock(paths)?;
        self.doctor_locked(paths, platform)
    }

    pub(super) fn doctor_locked(
        &self,
        paths: &RuntimePaths,
        platform: Platform,
    ) -> Result<DoctorReport, RuntimeError> {
        let installed = read_validated_installed(paths, &platform)?;
        let unit = verify_installed_unit(paths, &installed)?;
        let docker = run_checked(
            &self.runner,
            Path::new("docker"),
            &["version", "--format", "{{.Server.Version}}"],
        )?;
        self.verify_maia_assets(&installed)?;
        verify_stockfish_uci(&self.runner, &unit.stockfish())?;
        let status = self.maia_status_for(&installed)?;
        if status.state != MaiaServiceState::Running {
            return Err(RuntimeError::Unhealthy(
                "Maia service is not running".to_string(),
            ));
        }
        let health = docker_inspect(
            &self.runner,
            "{{.State.Health.Status}}",
            &installed.container_name(),
        )?;
        if health.trim() != "healthy" {
            return Err(RuntimeError::Unhealthy(format!(
                "Maia service health is {}",
                health.trim()
            )));
        }
        Ok(DoctorReport {
            unit_version: installed.manifest.unit_version,
            skill_sha256: installed.skill_sha256,
            stockfish_version: installed.manifest.stockfish.version,
            stockfish_archive_sha256: installed.manifest.stockfish.archive_sha256,
            stockfish_binary_sha256: installed.stockfish_binary_sha256,
            maia_image: installed.manifest.maia.image,
            maia_package: installed.manifest.maia.package,
            maia_model: installed.manifest.maia.model,
            maia_model_sha256: installed.manifest.maia.model_sha256,
            maia_config_sha256: installed.manifest.maia.config_sha256,
            maia_port: installed.manifest.maia.port,
            docker_version: docker.stdout.trim().to_string(),
            maia_state: status.state,
        })
    }

    pub fn review_config(
        &self,
        paths: &RuntimePaths,
        platform: Platform,
    ) -> Result<ReviewRuntimeLease, RuntimeError> {
        self.runtime_lease(paths, platform, RuntimeStartupMode::Reuse)
    }

    pub fn certification_config(
        &self,
        paths: &RuntimePaths,
        platform: Platform,
    ) -> Result<ReviewRuntimeLease, RuntimeError> {
        self.runtime_lease(paths, platform, RuntimeStartupMode::Restart)
    }

    pub fn engine_config(
        &self,
        paths: &RuntimePaths,
        platform: Platform,
    ) -> Result<EngineRuntimeConfig, RuntimeError> {
        let installed = read_validated_installed(paths, &platform)?;
        let stockfish_path = paths.unit(&installed.manifest.unit_version).stockfish();
        verify_sha256(
            &fs::read(&stockfish_path)?,
            &installed.stockfish_binary_sha256,
            "installed Stockfish binary",
        )?;
        Ok(EngineRuntimeConfig {
            unit_version: installed.manifest.unit_version,
            stockfish_path,
            stockfish_version: installed.manifest.stockfish.version,
        })
    }

    fn runtime_lease(
        &self,
        paths: &RuntimePaths,
        platform: Platform,
        startup_mode: RuntimeStartupMode,
    ) -> Result<ReviewRuntimeLease, RuntimeError> {
        let lock = acquire_runtime_lock(paths)?;
        let installed = read_validated_installed(paths, &platform)?;
        let stockfish_path = paths.unit(&installed.manifest.unit_version).stockfish();
        verify_sha256(
            &fs::read(&stockfish_path)?,
            &installed.stockfish_binary_sha256,
            "installed Stockfish binary",
        )?;
        self.ensure_docker()?;
        self.verify_maia_assets(&installed)?;
        if matches!(startup_mode, RuntimeStartupMode::Restart) {
            remove_owned_container(
                &self.runner,
                &installed.container_name(),
                &installed.owner_id,
            )?;
        }
        self.start_maia_for(&installed)?;
        let health = docker_inspect(
            &self.runner,
            "{{.State.Health.Status}}",
            &installed.container_name(),
        )?;
        if health.trim() != "healthy" {
            return Err(RuntimeError::Unhealthy(format!(
                "Maia service health is {}",
                health.trim()
            )));
        }
        let maia_container_name = installed.container_name();
        Ok(ReviewRuntimeLease {
            config: ReviewRuntimeConfig {
                unit_version: installed.manifest.unit_version,
                stockfish_path,
                stockfish_version: installed.manifest.stockfish.version,
                maia_base_url: format!("http://127.0.0.1:{}", installed.manifest.maia.port),
                maia_image: installed.manifest.maia.image,
                maia_package: installed.manifest.maia.package,
                maia_model: installed.manifest.maia.model,
                maia_model_sha256: installed.manifest.maia.model_sha256,
                maia_config_sha256: installed.manifest.maia.config_sha256,
                maia_container_name,
            },
            _lock: lock,
        })
    }

    pub fn maia_resources_for_lease(
        &self,
        lease: &ReviewRuntimeLease,
    ) -> Result<String, RuntimeError> {
        Ok(run_checked(
            &self.runner,
            Path::new("docker"),
            &[
                "stats",
                "--no-stream",
                "--format",
                "{{.MemUsage}}; {{.CPUPerc}}",
                &lease.config.maia_container_name,
            ],
        )?
        .stdout
        .trim()
        .to_string())
    }

    pub(super) fn installation_intact_locked(
        &self,
        paths: &RuntimePaths,
        platform: Platform,
    ) -> Result<(), RuntimeError> {
        let installed = read_validated_installed(paths, &platform)?;
        verify_installed_unit(paths, &installed)?;
        self.ensure_docker()?;
        self.verify_maia_assets(&installed)?;
        Ok(())
    }

    pub fn start_maia(&self, paths: &RuntimePaths) -> Result<MaiaServiceState, RuntimeError> {
        let _lock = acquire_runtime_lock(paths)?;
        self.ensure_docker()?;
        let installed = read_validated_installed(paths, &Platform::current())?;
        self.start_maia_for(&installed)
    }

    fn start_maia_for(
        &self,
        installed: &super::InstalledRuntime,
    ) -> Result<MaiaServiceState, RuntimeError> {
        if owned_container_running(
            &self.runner,
            &installed.container_name(),
            &installed.owner_id,
        )? {
            let health = docker_inspect(
                &self.runner,
                "{{.State.Health.Status}}",
                &installed.container_name(),
            )?;
            if health.trim() == "healthy" {
                return Ok(MaiaServiceState::Running);
            }
            remove_owned_container(
                &self.runner,
                &installed.container_name(),
                &installed.owner_id,
            )?;
        }
        start_container(
            &self.runner,
            &installed.manifest,
            &installed.owner_id,
            &installed.model_volume(),
            &installed.container_name(),
        )?;
        Ok(MaiaServiceState::Running)
    }

    pub fn stop_maia(&self, paths: &RuntimePaths) -> Result<MaiaServiceState, RuntimeError> {
        let _lock = acquire_runtime_lock(paths)?;
        self.ensure_docker()?;
        let installed = read_validated_installed(paths, &Platform::current())?;
        if !owned_container_running(
            &self.runner,
            &installed.container_name(),
            &installed.owner_id,
        )? {
            return Ok(MaiaServiceState::Stopped);
        }
        run_checked(
            &self.runner,
            Path::new("docker"),
            &["rm", "--force", &installed.container_name()],
        )?;
        Ok(MaiaServiceState::Stopped)
    }

    pub fn maia_status(&self, paths: &RuntimePaths) -> Result<MaiaStatus, RuntimeError> {
        let _lock = acquire_runtime_lock(paths)?;
        self.ensure_docker()?;
        let installed = read_validated_installed(paths, &Platform::current())?;
        self.maia_status_for(&installed)
    }

    pub fn uninstall(&self, paths: &RuntimePaths) -> Result<UninstallOutcome, RuntimeError> {
        let _lock = acquire_runtime_lock(paths)?;
        let installed = read_installed(paths)?;
        validate_uninstall_identity(&installed)?;
        let links = [
            paths.bin_link(),
            paths.codex_skill_link(),
            paths.claude_skill_link(),
        ];
        for link in &links {
            verify_owned_link_or_missing(link, &paths.data_dir().join("units"))?;
        }
        self.ensure_docker()?;
        remove_owned_container(
            &self.runner,
            &installed.container_name(),
            &installed.owner_id,
        )?;
        let mut volumes = runtime_unit_versions(paths)?
            .into_iter()
            .map(|version| super::model_volume(&version, &installed.owner_id))
            .collect::<HashSet<_>>();
        volumes.insert(installed.model_volume());
        let labeled = run_checked(
            &self.runner,
            Path::new("docker"),
            &[
                "volume",
                "ls",
                "--filter",
                &format!("label=io.chenchess.runtime={}", installed.owner_id),
                "--format",
                "{{.Name}}",
            ],
        )?;
        for volume in labeled
            .stdout
            .lines()
            .filter(|line| !line.trim().is_empty())
        {
            volumes.insert(volume.trim().to_string());
        }
        for volume in volumes {
            remove_volume_if_present(&self.runner, &volume)?;
        }
        super::activation::PreparedActivation::remove_artifacts(
            paths,
            super::active_unit_links(paths, &paths.unit(&installed.manifest.unit_version)),
        )?;
        for link in links {
            remove_owned_link_if_present(&link, &paths.data_dir().join("units"))?;
        }
        remove_file_if_present(&paths.config_file())?;
        match fs::remove_dir_all(paths.data_dir()) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(RuntimeError::Io(error)),
        }
        Ok(UninstallOutcome {
            unit_version: installed.manifest.unit_version,
        })
    }

    fn maia_status_for(
        &self,
        installed: &super::InstalledRuntime,
    ) -> Result<MaiaStatus, RuntimeError> {
        if !owned_container_running(
            &self.runner,
            &installed.container_name(),
            &installed.owner_id,
        )? {
            return Ok(MaiaStatus {
                state: MaiaServiceState::Stopped,
                unit_version: installed.manifest.unit_version.clone(),
                image: installed.manifest.maia.image.clone(),
                package: installed.manifest.maia.package.clone(),
                model: installed.manifest.maia.model.clone(),
                resources: None,
            });
        }
        let resources = run_checked(
            &self.runner,
            Path::new("docker"),
            &[
                "stats",
                "--no-stream",
                "--format",
                "{{.MemUsage}}; {{.CPUPerc}}",
                &installed.container_name(),
            ],
        )?
        .stdout
        .trim()
        .to_string();
        Ok(MaiaStatus {
            state: MaiaServiceState::Running,
            unit_version: installed.manifest.unit_version.clone(),
            image: installed.manifest.maia.image.clone(),
            package: installed.manifest.maia.package.clone(),
            model: installed.manifest.maia.model.clone(),
            resources: Some(resources),
        })
    }

    fn ensure_docker(&self) -> Result<(), RuntimeError> {
        run_checked(
            &self.runner,
            Path::new("docker"),
            &["version", "--format", "{{.Server.Version}}"],
        )?;
        Ok(())
    }

    fn verify_maia_assets(&self, installed: &super::InstalledRuntime) -> Result<(), RuntimeError> {
        run_checked(
            &self.runner,
            Path::new("docker"),
            &["image", "inspect", &installed.manifest.maia.image],
        )?;
        run_checked(
            &self.runner,
            Path::new("docker"),
            &["volume", "inspect", &installed.model_volume()],
        )?;
        run_checked_owned(
            &self.runner,
            Path::new("docker"),
            vec![
                "run".to_string(),
                "--rm".to_string(),
                "--pull=never".to_string(),
                "--volume".to_string(),
                format!("{}:/models", installed.model_volume()),
                installed.manifest.maia.image.clone(),
                "python".to_string(),
                "-c".to_string(),
                format!(
                    "from importlib.metadata import version; from pathlib import Path; import hashlib; package = 'maia2==' + version('maia2'); digest = lambda name: hashlib.sha256(Path('/models', name).read_bytes()).hexdigest(); assert package == {:?}; assert digest('rapid_model.pt') == {:?}; assert digest('config.yaml') == {:?}; assert Path('/models/.chenchess-package').read_text() == package; assert Path('/models/.chenchess-model').read_text() == {:?}",
                    installed.manifest.maia.package,
                    installed.manifest.maia.model_sha256,
                    installed.manifest.maia.config_sha256,
                    installed.manifest.maia.model,
                ),
            ],
        )?;
        Ok(())
    }
}

fn validate_uninstall_identity(installed: &super::InstalledRuntime) -> Result<(), RuntimeError> {
    validate_installed_identity(installed)
}

fn runtime_unit_versions(paths: &RuntimePaths) -> Result<Vec<String>, RuntimeError> {
    let entries = match fs::read_dir(paths.data_dir().join("units")) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(RuntimeError::Io(error)),
    };
    Ok(entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|version| super::valid_unit_version(version))
        .collect())
}

fn remove_volume_if_present<R: ProcessRunner>(
    runner: &R,
    volume: &str,
) -> Result<(), RuntimeError> {
    let inspect = runner
        .run(
            Path::new("docker"),
            &[
                "volume".to_string(),
                "inspect".to_string(),
                volume.to_string(),
            ],
        )
        .map_err(|message| RuntimeError::Process {
            program: "docker".to_string(),
            message,
        })?;
    if inspect.success {
        run_checked(runner, Path::new("docker"), &["volume", "rm", volume])?;
    } else if !inspect.stderr.to_ascii_lowercase().contains("not found")
        && !inspect
            .stderr
            .to_ascii_lowercase()
            .contains("no such volume")
    {
        return Err(RuntimeError::Process {
            program: "docker".to_string(),
            message: inspect.stderr,
        });
    }
    Ok(())
}

fn remove_owned_link_if_present(target: &Path, units: &Path) -> Result<(), RuntimeError> {
    if !verify_owned_link_or_missing(target, units)? {
        return Ok(());
    }
    fs::remove_file(target)?;
    Ok(())
}

fn verify_owned_link_or_missing(target: &Path, units: &Path) -> Result<bool, RuntimeError> {
    let metadata = match target.symlink_metadata() {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(RuntimeError::Io(error)),
    };
    if !metadata.file_type().is_symlink()
        || !fs::read_link(target).is_ok_and(|linked| linked.starts_with(units))
    {
        return Err(RuntimeError::PathConflict(target.to_path_buf()));
    }
    Ok(true)
}

fn remove_file_if_present(path: &Path) -> Result<(), RuntimeError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(RuntimeError::Io(error)),
    }
}

fn verify_installed_unit(
    paths: &RuntimePaths,
    installed: &super::InstalledRuntime,
) -> Result<super::RuntimeUnitPaths, RuntimeError> {
    let unit = paths.unit(&installed.manifest.unit_version);
    verify_install_link(&paths.bin_link(), &unit.cli())?;
    verify_install_link(&paths.codex_skill_link(), &unit.skill())?;
    verify_install_link(&paths.claude_skill_link(), &unit.skill())?;
    skill::verify(&unit.skill(), &installed.skill_sha256)?;
    verify_sha256(
        &fs::read(unit.cli())?,
        &installed.cli_sha256,
        "installed CLI",
    )?;
    verify_sha256(
        &fs::read(unit.stockfish())?,
        &installed.stockfish_binary_sha256,
        "installed Stockfish binary",
    )?;
    Ok(unit)
}

fn read_validated_installed(
    paths: &RuntimePaths,
    platform: &Platform,
) -> Result<super::InstalledRuntime, RuntimeError> {
    let installed = read_installed(paths)?;
    validate_installed(&installed, platform)?;
    Ok(installed)
}

fn validate_installed(
    installed: &super::InstalledRuntime,
    platform: &Platform,
) -> Result<(), RuntimeError> {
    if uuid::Uuid::parse_str(&installed.owner_id).is_err() {
        return Err(RuntimeError::InvalidManifest(
            "installed runtime owner ID is invalid".to_string(),
        ));
    }
    if installed.manifest.schema_version != super::RUNTIME_MANIFEST_SCHEMA_VERSION {
        return Err(RuntimeError::SchemaMismatch {
            installed: installed.manifest.schema_version,
            supported: super::RUNTIME_MANIFEST_SCHEMA_VERSION,
        });
    }
    validate_manifest(&installed.manifest, platform)?;
    Ok(())
}
