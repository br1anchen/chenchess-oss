use std::{
    fs::{self, File, OpenOptions},
    future::Future,
    io::Cursor,
    path::{Path, PathBuf},
    pin::Pin,
};

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
mod activation;
mod docker;
mod manager;
mod process;
mod skill;
mod state;

use activation::{ActivationLink, PreparedActivation};
use docker::{
    cleanup_container, docker_inspect, maia_container, model_volume, owned_container_running,
    remove_owned_container, start_container,
};
pub use manager::{
    DoctorReport, EngineRuntimeConfig, MaiaServiceState, MaiaStatus, ReviewRuntimeConfig,
    ReviewRuntimeLease, RuntimeManager, UninstallOutcome,
};
pub use process::{ProcessCancellation, SystemProcessRunner};
use state::{
    clear_pending_install, pending_install_owner, read_installed, read_installed_if_present,
    validate_installed_identity, InstalledRuntime,
};

const MAIA_PACKAGE_IDENTITY: &str = "maia2==0.11.0";
const MAIA_MODEL_IDENTITY: &str = "rapid";
pub const RUNTIME_MANIFEST_SCHEMA_VERSION: u8 = 1;

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeManifest {
    pub unit_version: String,
    pub schema_version: u8,
    pub target: RuntimeTarget,
    pub stockfish: StockfishPackage,
    pub maia: MaiaPackage,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
pub struct RuntimeTarget {
    pub os: String,
    pub arch: String,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StockfishPackage {
    pub version: String,
    pub url: String,
    pub archive_sha256: String,
    pub binary_member: String,
    pub license_members: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaiaPackage {
    pub image: String,
    pub package: String,
    pub model: String,
    pub model_sha256: String,
    pub config_sha256: String,
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Platform {
    os: String,
    arch: String,
}

impl Platform {
    pub fn new(os: impl Into<String>, arch: impl Into<String>) -> Self {
        Self {
            os: os.into(),
            arch: arch.into(),
        }
    }

    pub fn current() -> Self {
        Self::new(std::env::consts::OS, std::env::consts::ARCH)
    }
}

#[derive(Debug, Clone)]
pub struct RuntimePaths {
    home: PathBuf,
}

impl RuntimePaths {
    pub fn from_home(home: &Path) -> Self {
        Self {
            home: home.to_path_buf(),
        }
    }

    pub fn from_environment() -> Result<Self, RuntimeError> {
        let home = std::env::var_os("HOME")
            .filter(|value| !value.is_empty())
            .ok_or_else(|| RuntimeError::InvalidEnvironment("HOME is not set".to_string()))?;
        Ok(Self::from_home(Path::new(&home)))
    }

    pub fn unit(&self, version: &str) -> RuntimeUnitPaths {
        RuntimeUnitPaths {
            root: self.data_dir().join("units").join(version),
        }
    }

    pub fn bin_link(&self) -> PathBuf {
        self.home.join(".local/bin/chenchess")
    }

    pub fn config_file(&self) -> PathBuf {
        self.home.join(".config/chenchess/runtime.json")
    }

    pub fn state_dir(&self) -> PathBuf {
        self.home.join(".local/state/chenchess")
    }

    pub fn codex_skill_link(&self) -> PathBuf {
        self.home.join(".agents/skills/chenchess-coach")
    }

    pub fn claude_skill_link(&self) -> PathBuf {
        self.home.join(".claude/skills/chenchess-coach")
    }

    pub fn is_installed_executable(&self, executable: &Path) -> Result<bool, RuntimeError> {
        let executable = fs::canonicalize(executable)?;
        if fs::canonicalize(self.bin_link()).is_ok_and(|entrypoint| entrypoint == executable) {
            return Ok(true);
        }

        let units = match fs::canonicalize(self.data_dir().join("units")) {
            Ok(path) => path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(RuntimeError::Io(error)),
        };
        if !executable.starts_with(units) {
            return Ok(false);
        }
        let Some(installed) = read_installed_if_present(self)? else {
            return Ok(false);
        };
        Ok(fs::canonicalize(self.unit(&installed.manifest.unit_version).cli())? == executable)
    }

    fn pending_install_file(&self) -> PathBuf {
        self.state_dir().join("pending-install.json")
    }

    pub(super) fn data_dir(&self) -> PathBuf {
        self.home.join(".local/share/chenchess")
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeUnitPaths {
    root: PathBuf,
}

impl RuntimeUnitPaths {
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn cli(&self) -> PathBuf {
        self.root.join("bin/chenchess")
    }

    pub fn stockfish(&self) -> PathBuf {
        self.root.join("bin/stockfish")
    }

    pub fn stockfish_licenses(&self) -> PathBuf {
        self.root.join("licenses/stockfish")
    }

    pub fn third_party_notices(&self) -> PathBuf {
        self.root.join("licenses/THIRD_PARTY_NOTICES.md")
    }

    pub fn skill(&self) -> PathBuf {
        self.root.join("skills/chenchess-coach")
    }
}

pub struct InstallRequest {
    pub manifest: RuntimeManifest,
    pub paths: RuntimePaths,
    pub source_cli: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InstallOutcome {
    Installed,
    Updated,
    AlreadyInstalled,
}

pub trait DownloadClient: Send + Sync {
    fn download<'a>(
        &'a self,
        url: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, String>> + Send + 'a>>;
}

pub trait ProcessRunner: Send + Sync {
    fn run(&self, program: &Path, args: &[String]) -> Result<ProcessOutput, String>;

    fn run_with_input(
        &self,
        program: &Path,
        args: &[String],
        input: &str,
    ) -> Result<ProcessOutput, String>;
}

#[derive(Debug)]
pub struct ProcessOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

pub struct RuntimeInstaller<D, R> {
    downloader: D,
    runner: R,
}

impl<D, R> RuntimeInstaller<D, R>
where
    D: DownloadClient,
    R: ProcessRunner,
{
    pub fn new(downloader: D, runner: R) -> Self {
        Self { downloader, runner }
    }

    pub async fn install(&self, request: InstallRequest) -> Result<InstallOutcome, RuntimeError> {
        let platform = Platform::current();
        validate_manifest(&request.manifest, &platform)?;
        let _lock = acquire_runtime_lock(&request.paths)?;
        PreparedActivation::recover_config(&request.paths)?;
        let mut previous = read_installed_if_present(&request.paths)?;
        if PreparedActivation::has_config_backup(&request.paths)? {
            let activation_finished = match &previous {
                Some(installed) => PreparedActivation::active_links_match(&active_unit_links(
                    &request.paths,
                    &request.paths.unit(&installed.manifest.unit_version),
                ))?,
                None => false,
            };
            if !activation_finished {
                PreparedActivation::restore_config_backup(&request.paths)?;
                previous = read_installed_if_present(&request.paths)?;
            }
        }
        if let Some(installed) = &previous {
            validate_installed_identity(installed)?;
            PreparedActivation::recover_active(
                &request.paths,
                active_unit_links(
                    &request.paths,
                    &request.paths.unit(&installed.manifest.unit_version),
                ),
            )?;
        }
        if let Some(installed) = previous.as_ref().filter(|installed| {
            installed.manifest.unit_version == request.manifest.unit_version
                && installed.manifest != request.manifest
        }) {
            return Err(RuntimeError::UnitVersionConflict {
                version: installed.manifest.unit_version.clone(),
            });
        }
        if previous
            .as_ref()
            .is_some_and(|installed| installed.manifest == request.manifest)
        {
            if RuntimeManager::new(&self.runner)
                .installation_intact_locked(&request.paths, platform.clone())
                .is_ok()
            {
                clear_pending_install(&request.paths)?;
                return Ok(InstallOutcome::AlreadyInstalled);
            }
            clear_pending_install(&request.paths)?;
            return Err(RuntimeError::DamagedInstallation {
                version: request.manifest.unit_version.clone(),
            });
        }

        let unit = request.paths.unit(&request.manifest.unit_version);
        let unit_links = active_unit_links(&request.paths, &unit);
        let previous_links = previous.as_ref().map(|installed| {
            active_unit_links(
                &request.paths,
                &request.paths.unit(&installed.manifest.unit_version),
            )
        });
        for (index, link) in unit_links.iter().enumerate() {
            ensure_install_link_available(
                &link.target,
                &link.source,
                previous_links
                    .as_ref()
                    .and_then(|links| links.get(index))
                    .map(|link| link.source.as_path()),
            )?;
        }

        let owner_id = previous.as_ref().map_or_else(
            || pending_install_owner(&request.paths, &request.manifest.unit_version),
            |installed| Ok(installed.owner_id.clone()),
        )?;

        run_checked(
            &self.runner,
            Path::new("docker"),
            &["version", "--format", "{{.Server.Version}}"],
        )?;

        let archive = self
            .downloader
            .download(&request.manifest.stockfish.url)
            .await
            .map_err(RuntimeError::Download)?;
        verify_sha256(
            &archive,
            &request.manifest.stockfish.archive_sha256,
            "Stockfish archive",
        )?;

        fs::create_dir_all(unit.cli().parent().expect("CLI path has a parent"))?;
        fs::create_dir_all(unit.stockfish_licenses())?;
        fs::create_dir_all(request.paths.state_dir())?;
        fs::create_dir_all(
            request
                .paths
                .config_file()
                .parent()
                .expect("config path has a parent"),
        )?;
        for link in &unit_links {
            fs::create_dir_all(link.target.parent().expect("unit link has a parent"))?;
        }

        extract_stockfish(&archive, &request.manifest.stockfish, &unit)?;
        fs::write(
            unit.third_party_notices(),
            include_str!("../../../runtime/THIRD_PARTY_NOTICES.md"),
        )?;
        copy_cli_if_distinct(&request.source_cli, &unit.cli())?;
        skill::install(&unit.skill())?;
        verify_stockfish_uci(&self.runner, &unit.stockfish())?;

        run_checked(
            &self.runner,
            Path::new("docker"),
            &["pull", &request.manifest.maia.image],
        )?;
        let volume = model_volume(&request.manifest.unit_version, &owner_id);
        run_checked(
            &self.runner,
            Path::new("docker"),
            &[
                "volume",
                "create",
                "--label",
                &format!("io.chenchess.runtime={owner_id}"),
                &volume,
            ],
        )?;
        let model_script = format!(
            "from importlib.metadata import version\nfrom importlib.resources import as_file, files\nfrom pathlib import Path\nimport hashlib\nfrom maia2 import model\npackage = 'maia2==' + version('maia2')\nassert package == {:?}\nmodel.from_pretrained(type={:?}, device='cpu', save_root='/models')\nwith as_file(files('maia2.configs').joinpath('maia2-training.yaml')) as source: Path('/models/config.yaml').write_bytes(Path(source).read_bytes())\ndef digest(name): return hashlib.sha256(Path('/models', name).read_bytes()).hexdigest()\nassert digest('rapid_model.pt') == {:?}\nassert digest('config.yaml') == {:?}\nPath('/models/.chenchess-package').write_text(package)\nPath('/models/.chenchess-model').write_text({:?})",
            request.manifest.maia.package,
            request.manifest.maia.model,
            request.manifest.maia.model_sha256,
            request.manifest.maia.config_sha256,
            request.manifest.maia.model
        );
        run_checked_owned(
            &self.runner,
            Path::new("docker"),
            vec![
                "run".to_string(),
                "--rm".to_string(),
                "--pull=never".to_string(),
                "--label".to_string(),
                "io.chenchess.runtime=1".to_string(),
                "--volume".to_string(),
                format!("{volume}:/models"),
                request.manifest.maia.image.clone(),
                "python".to_string(),
                "-c".to_string(),
                model_script,
            ],
        )?;

        let container_name = maia_container(&request.manifest.unit_version, &owner_id);
        let installed = InstalledRuntime {
            manifest: request.manifest.clone(),
            cli_sha256: sha256_file(&unit.cli())?,
            skill_sha256: skill::sha256(&unit.skill())?,
            stockfish_binary_sha256: sha256_file(&unit.stockfish())?,
            owner_id,
        };
        let activation = PreparedActivation::prepare(&request.paths, &installed, unit_links)?;

        if let Some(previous) = &previous {
            remove_owned_container(&self.runner, &previous.container_name(), &previous.owner_id)?;
        }

        if let Err(error) = start_container(
            &self.runner,
            &request.manifest,
            &installed.owner_id,
            &volume,
            &container_name,
        ) {
            return Err(rollback_install_failure(
                &self.runner,
                previous.as_ref(),
                &installed,
                error,
            ));
        }
        if let Err(error) = verify_live_review_session_smoke(
            &self.runner,
            &unit.cli(),
            &unit.stockfish(),
            request.manifest.maia.port,
        ) {
            return Err(rollback_install_failure(
                &self.runner,
                previous.as_ref(),
                &installed,
                error,
            ));
        }

        if let Err(error) = activation.commit() {
            return Err(rollback_install_failure(
                &self.runner,
                previous.as_ref(),
                &installed,
                error,
            ));
        }
        clear_pending_install(&request.paths)?;

        Ok(if previous.is_some() {
            InstallOutcome::Updated
        } else {
            InstallOutcome::Installed
        })
    }
}

fn active_unit_links(paths: &RuntimePaths, unit: &RuntimeUnitPaths) -> Vec<ActivationLink> {
    vec![
        ActivationLink::file(paths.bin_link(), unit.cli()),
        ActivationLink::directory(paths.codex_skill_link(), unit.skill()),
        ActivationLink::directory(paths.claude_skill_link(), unit.skill()),
    ]
}

impl<T: ProcessRunner + ?Sized> ProcessRunner for &T {
    fn run(&self, program: &Path, args: &[String]) -> Result<ProcessOutput, String> {
        (**self).run(program, args)
    }

    fn run_with_input(
        &self,
        program: &Path,
        args: &[String],
        input: &str,
    ) -> Result<ProcessOutput, String> {
        (**self).run_with_input(program, args, input)
    }
}

fn validate_manifest(manifest: &RuntimeManifest, platform: &Platform) -> Result<(), RuntimeError> {
    if manifest.target.os != platform.os || manifest.target.arch != platform.arch {
        return Err(RuntimeError::UnsupportedPlatform {
            expected_os: manifest.target.os.clone(),
            expected_arch: manifest.target.arch.clone(),
            actual_os: platform.os.clone(),
            actual_arch: platform.arch.clone(),
        });
    }
    if manifest.schema_version != RUNTIME_MANIFEST_SCHEMA_VERSION {
        return Err(RuntimeError::InvalidManifest(format!(
            "schema version {} is incompatible with supported version {}",
            manifest.schema_version, RUNTIME_MANIFEST_SCHEMA_VERSION
        )));
    }
    if !valid_unit_version(&manifest.unit_version) {
        return Err(RuntimeError::InvalidManifest(
            "unit version must be at most 128 characters, start with a lowercase ASCII letter or digit, and contain only lowercase ASCII letters, digits, '.', '_', and '-'"
                .to_string(),
        ));
    }
    if !manifest.stockfish.url.starts_with("https://") {
        return Err(RuntimeError::InvalidManifest(
            "Stockfish URL must use HTTPS".to_string(),
        ));
    }
    if !valid_sha256(&manifest.stockfish.archive_sha256) {
        return Err(RuntimeError::InvalidManifest(
            "Stockfish archive SHA-256 must be 64 lowercase hexadecimal characters".to_string(),
        ));
    }
    let digest = manifest
        .maia
        .image
        .split_once("@sha256:")
        .map(|(_, digest)| digest)
        .filter(|digest| valid_sha256(digest));
    if digest.is_none() {
        return Err(RuntimeError::InvalidManifest(
            "Maia image must use an immutable @sha256 digest".to_string(),
        ));
    }
    if manifest.stockfish.license_members.is_empty() {
        return Err(RuntimeError::InvalidManifest(
            "Stockfish license members must not be empty".to_string(),
        ));
    }
    if manifest.maia.package != MAIA_PACKAGE_IDENTITY {
        return Err(RuntimeError::InvalidManifest(format!(
            "Maia package identity must be {MAIA_PACKAGE_IDENTITY}"
        )));
    }
    if manifest.maia.model != MAIA_MODEL_IDENTITY {
        return Err(RuntimeError::InvalidManifest(format!(
            "Maia model identity must be {MAIA_MODEL_IDENTITY}"
        )));
    }
    if !valid_sha256(&manifest.maia.model_sha256) || !valid_sha256(&manifest.maia.config_sha256) {
        return Err(RuntimeError::InvalidManifest(
            "Maia model and config SHA-256 values must be 64 lowercase hexadecimal characters"
                .to_string(),
        ));
    }
    if manifest.maia.port == 0 {
        return Err(RuntimeError::InvalidManifest(
            "Maia port must be nonzero".to_string(),
        ));
    }
    Ok(())
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_unit_version(value: &str) -> bool {
    if value.len() > 128 {
        return false;
    }
    let mut bytes = value.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && bytes.all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

fn extract_stockfish(
    archive: &[u8],
    package: &StockfishPackage,
    unit: &RuntimeUnitPaths,
) -> Result<(), RuntimeError> {
    let mut binary_found = false;
    let mut remaining_licenses = package
        .license_members
        .iter()
        .cloned()
        .collect::<std::collections::HashSet<_>>();
    for entry in tar::Archive::new(Cursor::new(archive)).entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_string_lossy().to_string();
        if path == package.binary_member {
            entry.unpack(unit.stockfish())?;
            binary_found = true;
        } else if remaining_licenses.remove(&path) {
            let name = Path::new(&path)
                .file_name()
                .ok_or_else(|| RuntimeError::InvalidArchive(path.clone()))?;
            entry.unpack(unit.stockfish_licenses().join(name))?;
        }
    }
    if !binary_found {
        return Err(RuntimeError::InvalidArchive(format!(
            "missing Stockfish binary member {}",
            package.binary_member
        )));
    }
    if !remaining_licenses.is_empty() {
        return Err(RuntimeError::InvalidArchive(format!(
            "missing Stockfish license members: {}",
            remaining_licenses
                .into_iter()
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    Ok(())
}

fn cleanup_install_failure<R: ProcessRunner>(
    runner: &R,
    installed: &InstalledRuntime,
    cause: RuntimeError,
) -> RuntimeError {
    match cleanup_container(runner, installed) {
        Ok(()) => cause,
        Err(cleanup) => RuntimeError::ContainerCleanup {
            cause: cause.to_string(),
            cleanup: cleanup.to_string(),
        },
    }
}

fn rollback_install_failure<R: ProcessRunner>(
    runner: &R,
    previous: Option<&InstalledRuntime>,
    replacement: &InstalledRuntime,
    cause: RuntimeError,
) -> RuntimeError {
    let Some(previous) = previous else {
        return cleanup_install_failure(runner, replacement, cause);
    };
    let cleanup = cleanup_container(runner, replacement).err();
    let restart = start_container(
        runner,
        &previous.manifest,
        &previous.owner_id,
        &previous.model_volume(),
        &previous.container_name(),
    )
    .err();
    match (cleanup, restart) {
        (None, None) => cause,
        (cleanup, restart) => RuntimeError::UpdateRollback {
            cause: cause.to_string(),
            rollback: [cleanup, restart]
                .into_iter()
                .flatten()
                .map(|error| error.to_string())
                .collect::<Vec<_>>()
                .join("; "),
        },
    }
}

fn verify_install_link(target: &Path, installed: &Path) -> Result<(), RuntimeError> {
    let metadata = target
        .symlink_metadata()
        .map_err(|_| RuntimeError::PathConflict(target.to_path_buf()))?;
    if metadata.file_type().is_symlink() && fs::read_link(target).ok().as_deref() == Some(installed)
    {
        Ok(())
    } else {
        Err(RuntimeError::PathConflict(target.to_path_buf()))
    }
}

struct RuntimeMutationLock {
    file: File,
}

impl Drop for RuntimeMutationLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

fn acquire_runtime_lock(paths: &RuntimePaths) -> Result<RuntimeMutationLock, RuntimeError> {
    fs::create_dir_all(paths.state_dir())?;
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(paths.state_dir().join("runtime.lock"))?;
    match file.try_lock_exclusive() {
        Ok(()) => Ok(RuntimeMutationLock { file }),
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
            Err(RuntimeError::MutationInProgress)
        }
        Err(error) => Err(RuntimeError::Io(error)),
    }
}

fn verify_sha256(bytes: &[u8], expected: &str, label: &str) -> Result<(), RuntimeError> {
    let actual = format!("{:x}", Sha256::digest(bytes));
    if actual == expected.to_ascii_lowercase() {
        Ok(())
    } else {
        Err(RuntimeError::Checksum {
            label: label.to_string(),
            expected: expected.to_string(),
            actual,
        })
    }
}

fn verify_stockfish_uci<R: ProcessRunner>(
    runner: &R,
    stockfish: &Path,
) -> Result<(), RuntimeError> {
    let output = runner
        .run_with_input(stockfish, &[], "uci\nquit\n")
        .map_err(|message| RuntimeError::Process {
            program: stockfish.display().to_string(),
            message,
        })?;
    if output.success && output.stdout.lines().any(|line| line.trim() == "uciok") {
        Ok(())
    } else {
        Err(RuntimeError::Unhealthy(
            "Stockfish did not complete the UCI handshake".to_string(),
        ))
    }
}

fn verify_live_review_session_smoke<R: ProcessRunner>(
    runner: &R,
    cli: &Path,
    stockfish: &Path,
    maia_port: u16,
) -> Result<(), RuntimeError> {
    let args = vec![
        format!("STOCKFISH_PATH={}", stockfish.display()),
        format!("MAIA_BASE_URL=http://127.0.0.1:{maia_port}"),
        "STOCKFISH_DEPTH=1".to_string(),
        cli.display().to_string(),
        "review-session".to_string(),
        "--jsonl".to_string(),
    ];
    let command = serde_json::json!({
        "requestId": "request:runtime-install-smoke",
        "operationId": "operation:runtime-install-smoke",
        "surface": "coachSkill",
        "command": {
            "kind": "importGame",
            "source": {
                "kind": "pastedPgn",
                "pgn": "[Event \"Runtime install smoke\"]\n[Result \"0-1\"]\n\n1. f3 e5 2. g4 Qh4# 0-1\n"
            },
            "reviewSide": { "kind": "selected", "reviewSide": "both" },
            "eloProfile": { "kind": "playerProvided", "rating": 1500 }
        }
    });
    let output = runner
        .run_with_input(Path::new("/usr/bin/env"), &args, &format!("{}\n", command))
        .map_err(|message| RuntimeError::Process {
            program: cli.display().to_string(),
            message,
        })?;
    if !output.success {
        return Err(RuntimeError::Unhealthy(format!(
            "live Review Session smoke failed: {}",
            output.stderr.trim()
        )));
    }
    let terminal = output.stdout.lines().last().ok_or_else(|| {
        RuntimeError::Unhealthy("live Review Session smoke returned no events".to_string())
    })?;
    let terminal: crate::review_session_contract::ReviewSessionEventEnvelope =
        serde_json::from_str(terminal).map_err(|error| {
            RuntimeError::Unhealthy(format!(
                "live Review Session smoke returned invalid JSONL: {error}"
            ))
        })?;
    let completed = match terminal.event {
        crate::review_session_contract::ReviewSessionEvent::Completed { result } => match *result {
            crate::review_session_contract::OperationCompletion::GameImported {
                review, ..
            } => !review.summary.trim().is_empty(),
            _ => false,
        },
        _ => false,
    };
    if !completed {
        return Err(RuntimeError::Unhealthy(
            "live Review Session smoke did not complete a grounded Game import".to_string(),
        ));
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, RuntimeError> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
}

fn copy_cli_if_distinct(source: &Path, target: &Path) -> Result<(), RuntimeError> {
    if !same_file(source, target) {
        fs::copy(source, target)?;
    }
    Ok(())
}

fn same_file(left: &Path, right: &Path) -> bool {
    left.canonicalize()
        .ok()
        .zip(right.canonicalize().ok())
        .is_some_and(|(left, right)| left == right)
}

fn run_checked<R: ProcessRunner>(
    runner: &R,
    program: &Path,
    args: &[&str],
) -> Result<ProcessOutput, RuntimeError> {
    run_checked_owned(
        runner,
        program,
        args.iter().map(|value| (*value).to_string()).collect(),
    )
}

fn run_checked_owned<R: ProcessRunner>(
    runner: &R,
    program: &Path,
    args: Vec<String>,
) -> Result<ProcessOutput, RuntimeError> {
    let output = runner
        .run(program, &args)
        .map_err(|message| RuntimeError::Process {
            program: program.display().to_string(),
            message,
        })?;
    if output.success {
        Ok(output)
    } else {
        Err(RuntimeError::Process {
            program: program.display().to_string(),
            message: output.stderr,
        })
    }
}

fn ensure_install_link_available(
    target: &Path,
    installed: &Path,
    previous: Option<&Path>,
) -> Result<(), RuntimeError> {
    let metadata = match target.symlink_metadata() {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(RuntimeError::Io(error)),
    };
    if metadata.file_type().is_symlink() {
        let linked = fs::read_link(target).ok();
        if linked.as_deref() == Some(installed) || linked.as_deref() == previous {
            return Ok(());
        }
    }
    Err(RuntimeError::PathConflict(target.to_path_buf()))
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error(
        "unsupported runtime platform {actual_os}/{actual_arch}; manifest requires {expected_os}/{expected_arch}"
    )]
    UnsupportedPlatform {
        expected_os: String,
        expected_arch: String,
        actual_os: String,
        actual_arch: String,
    },
    #[error("invalid runtime manifest: {0}")]
    InvalidManifest(String),
    #[error("runtime download failed: {0}")]
    Download(String),
    #[error("{label} checksum mismatch: expected {expected}, got {actual}")]
    Checksum {
        label: String,
        expected: String,
        actual: String,
    },
    #[error("invalid Stockfish archive: {0}")]
    InvalidArchive(String),
    #[error("{program} failed: {message}")]
    Process { program: String, message: String },
    #[error("installed schema {installed} is incompatible with supported schema {supported}")]
    SchemaMismatch { installed: u8, supported: u8 },
    #[error("runtime is unhealthy: {0}")]
    Unhealthy(String),
    #[error("path conflict at {0}; refusing to replace an unrelated installation")]
    PathConflict(PathBuf),
    #[error("container name conflict for {name}; existing owner label is {owner:?}")]
    ContainerNameConflict { name: String, owner: String },
    #[error("another runtime mutation is already in progress")]
    MutationInProgress,
    #[error("unit version conflict for {version}; installed runtime manifests are immutable")]
    UnitVersionConflict { version: String },
    #[error(
        "installed runtime {version} is damaged; run doctor for details and use the atomic reinstall workflow"
    )]
    DamagedInstallation { version: String },
    #[error("activation failed: {activation}; rollback also failed: {rollback}")]
    ActivationRollback {
        activation: String,
        rollback: String,
    },
    #[error("installation failed: {cause}; container cleanup also failed: {cleanup}")]
    ContainerCleanup { cause: String, cleanup: String },
    #[error("runtime update failed: {cause}; rollback also failed: {rollback}")]
    UpdateRollback { cause: String, rollback: String },
    #[error("invalid runtime environment: {0}")]
    InvalidEnvironment(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[derive(Clone)]
pub struct ReqwestDownloadClient {
    client: reqwest::Client,
}

impl Default for ReqwestDownloadClient {
    fn default() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl DownloadClient for ReqwestDownloadClient {
    fn download<'a>(
        &'a self,
        url: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, String>> + Send + 'a>> {
        Box::pin(async move {
            let response = self
                .client
                .get(url)
                .send()
                .await
                .map_err(|error| error.to_string())?
                .error_for_status()
                .map_err(|error| error.to_string())?;
            response
                .bytes()
                .await
                .map(|bytes| bytes.to_vec())
                .map_err(|error| error.to_string())
        })
    }
}
