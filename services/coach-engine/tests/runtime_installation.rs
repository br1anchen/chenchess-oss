#![cfg(unix)]

use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    future::Future,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, Mutex},
};

use chen_chess_coach_engine::local_runtime::{
    DownloadClient, InstallOutcome, InstallRequest, MaiaServiceState, Platform, ProcessOutput,
    ProcessRunner, RuntimeInstaller, RuntimeManager, RuntimeManifest, RuntimePaths,
};
use fs2::FileExt;
use sha2::{Digest, Sha256};

#[tokio::test]
async fn installs_a_checkout_independent_pinned_runtime_in_user_locations() {
    let root = test_root("install");
    fs::remove_dir_all(&root).ok();
    fs::create_dir_all(&root).expect("test root should be created");
    let source_cli = root.join("checkout/target/chenchess");
    fs::create_dir_all(source_cli.parent().expect("CLI should have a parent"))
        .expect("checkout target should be created");
    fs::write(&source_cli, b"test CLI").expect("source CLI should be written");
    executable(&source_cli);
    let archive = stockfish_archive();
    let manifest = manifest(hex_sha256(&archive));
    let downloader =
        FakeDownloader::new(HashMap::from([(manifest.stockfish.url.clone(), archive)]));
    let runner = RecordingRunner::healthy();
    let paths = RuntimePaths::from_home(&root.join("home"));
    let installer = RuntimeInstaller::new(downloader.clone(), runner.clone());

    let outcome = installer
        .install(InstallRequest {
            manifest: manifest.clone(),
            paths: paths.clone(),
            source_cli: source_cli.clone(),
        })
        .await
        .expect("pinned runtime should install");

    assert_eq!(outcome, InstallOutcome::Installed);
    let unit = paths.unit(&manifest.unit_version);
    assert_eq!(fs::read(unit.cli()).unwrap(), b"test CLI");
    assert_eq!(fs::read(unit.stockfish()).unwrap(), b"fake stockfish");
    assert!(unit.skill().join("SKILL.md").is_file());
    assert!(unit.skill().join("review-writing.md").is_file());
    assert!(unit.skill().join("review-session.md").is_file());
    assert_eq!(
        fs::read(unit.stockfish_licenses().join("Copying.txt")).unwrap(),
        b"GPL notice"
    );
    let notices = fs::read_to_string(unit.third_party_notices())
        .expect("runtime third-party notices should be installed");
    assert!(notices.contains("Stockfish"));
    assert!(notices.contains("Maia-2"));
    assert!(paths.bin_link().is_symlink());
    assert_eq!(
        fs::read_link(paths.codex_skill_link()).unwrap(),
        unit.skill()
    );
    assert_eq!(
        fs::read_link(paths.claude_skill_link()).unwrap(),
        unit.skill()
    );
    assert!(paths.config_file().is_file());
    let active: serde_json::Value =
        serde_json::from_slice(&fs::read(paths.config_file()).unwrap()).unwrap();
    assert_eq!(active["skillSha256"].as_str().unwrap().len(), 64);
    assert!(paths.state_dir().is_dir());
    let calls = runner.calls();
    assert!(calls.contains(&vec![
        "version".into(),
        "--format".into(),
        "{{.Server.Version}}".into()
    ]));
    assert!(calls.contains(&vec!["pull".into(), manifest.maia.image.clone()]));
    assert!(calls.iter().any(|args| {
        args.first().is_some_and(|arg| arg == "run")
            && args.iter().any(|arg| arg == &manifest.maia.image)
            && args.iter().any(|arg| arg.contains(&manifest.maia.package))
            && args.iter().any(|arg| arg.contains(&manifest.maia.model))
            && args.iter().any(|arg| arg.contains("maia2-training.yaml"))
    }));

    fs::remove_dir_all(source_cli.parent().unwrap().parent().unwrap())
        .expect("source checkout should be removable");
    assert_eq!(fs::read(unit.cli()).unwrap(), b"test CLI");
    assert_eq!(fs::read(unit.stockfish()).unwrap(), b"fake stockfish");
    assert!(unit.skill().join("SKILL.md").is_file());
    fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn rejects_unsupported_platform_missing_docker_and_bad_checksum_before_installation() {
    let root = test_root("prerequisites");
    fs::remove_dir_all(&root).ok();
    fs::create_dir_all(&root).unwrap();
    let source_cli = root.join("chenchess-source");
    fs::write(&source_cli, b"CLI").unwrap();
    let archive = stockfish_archive();
    let base_manifest = manifest(hex_sha256(&archive));
    let paths = RuntimePaths::from_home(&root.join("home"));

    let downloader = FakeDownloader::new(HashMap::from([(
        base_manifest.stockfish.url.clone(),
        archive.clone(),
    )]));
    let runner = RecordingRunner::healthy();
    let mut unsupported_manifest = base_manifest.clone();
    unsupported_manifest.target.os = "unsupported-os".to_string();
    let error = RuntimeInstaller::new(downloader.clone(), runner.clone())
        .install(InstallRequest {
            manifest: unsupported_manifest,
            paths: paths.clone(),
            source_cli: source_cli.clone(),
        })
        .await
        .expect_err("unsupported platform must fail before provisioning");
    assert!(error.to_string().contains("unsupported runtime platform"));
    assert!(downloader.calls().is_empty());
    assert!(runner.calls().is_empty());

    for (label, mut invalid) in [
        ("unsafe unit path", base_manifest.clone()),
        ("non-canonical unit version", base_manifest.clone()),
        ("schema mismatch", base_manifest.clone()),
        ("mutable image tag", base_manifest.clone()),
    ] {
        match label {
            "unsafe unit path" => invalid.unit_version = "../escape".to_string(),
            "non-canonical unit version" => invalid.unit_version = "release+candidate".to_string(),
            "schema mismatch" => invalid.schema_version = 99,
            "mutable image tag" => invalid.maia.image = "maia-runtime:latest".to_string(),
            _ => unreachable!(),
        }
        let downloader = FakeDownloader::new(HashMap::new());
        let runner = RecordingRunner::healthy();
        let error = RuntimeInstaller::new(downloader.clone(), runner.clone())
            .install(InstallRequest {
                manifest: invalid,
                paths: paths.clone(),
                source_cli: source_cli.clone(),
            })
            .await
            .expect_err(label);
        assert!(
            error.to_string().contains("invalid runtime manifest"),
            "{label}: {error}"
        );
        assert!(downloader.calls().is_empty());
        assert!(runner.calls().is_empty());
    }

    let downloader = FakeDownloader::new(HashMap::from([(
        base_manifest.stockfish.url.clone(),
        archive.clone(),
    )]));
    let error = RuntimeInstaller::new(downloader.clone(), RecordingRunner::unavailable())
        .install(InstallRequest {
            manifest: base_manifest.clone(),
            paths: paths.clone(),
            source_cli: source_cli.clone(),
        })
        .await
        .expect_err("missing Docker must fail before downloading");
    assert!(error.to_string().contains("docker failed"));
    assert!(downloader.calls().is_empty());

    let mut bad_manifest = base_manifest;
    bad_manifest.stockfish.archive_sha256 = "0".repeat(64);
    let downloader = FakeDownloader::new(HashMap::from([(
        bad_manifest.stockfish.url.clone(),
        archive,
    )]));
    let runner = RecordingRunner::healthy();
    let error = RuntimeInstaller::new(downloader, runner.clone())
        .install(InstallRequest {
            manifest: bad_manifest,
            paths: paths.clone(),
            source_cli,
        })
        .await
        .expect_err("checksum mismatch must fail before Docker pull");
    assert!(error.to_string().contains("checksum mismatch"));
    assert!(!runner
        .calls()
        .iter()
        .any(|args| args.first().is_some_and(|arg| arg == "pull")));
    assert!(!paths.config_file().exists());
    fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn healthy_reinstall_is_idempotent_and_damaged_units_require_atomic_reinstall() {
    let root = test_root("idempotent");
    fs::remove_dir_all(&root).ok();
    fs::create_dir_all(&root).unwrap();
    let source_cli = root.join("chenchess-source");
    fs::write(&source_cli, b"CLI").unwrap();
    executable(&source_cli);
    let archive = stockfish_archive();
    let manifest = manifest(hex_sha256(&archive));
    let downloader =
        FakeDownloader::new(HashMap::from([(manifest.stockfish.url.clone(), archive)]));
    let runner = StatefulRunner::new();
    let installer = RuntimeInstaller::new(downloader.clone(), runner.clone());
    let request = || InstallRequest {
        manifest: manifest.clone(),
        paths: RuntimePaths::from_home(&root.join("home")),
        source_cli: source_cli.clone(),
    };
    assert_eq!(
        installer.install(request()).await.unwrap(),
        InstallOutcome::Installed
    );

    let mut conflicting = manifest.clone();
    conflicting.maia.model_sha256 = "d".repeat(64);
    let download_count = downloader.calls().len();
    let process_count = runner.calls().len();
    let error = installer
        .install(InstallRequest {
            manifest: conflicting,
            paths: RuntimePaths::from_home(&root.join("home")),
            source_cli: source_cli.clone(),
        })
        .await
        .expect_err("an installed unit version must have one immutable manifest");
    assert!(error.to_string().contains("unit version conflict"));
    assert_eq!(downloader.calls().len(), download_count);
    assert_eq!(runner.calls().len(), process_count);

    assert_eq!(
        installer.install(request()).await.unwrap(),
        InstallOutcome::AlreadyInstalled
    );
    assert_eq!(downloader.calls().len(), 1);
    assert_eq!(
        runner
            .calls()
            .iter()
            .filter(|call| call.args.first().is_some_and(|arg| arg == "pull"))
            .count(),
        1
    );

    let paths = RuntimePaths::from_home(&root.join("home"));
    fs::remove_file(paths.bin_link()).unwrap();
    let error = installer.install(request()).await.unwrap_err();
    assert!(error.to_string().contains("is damaged"));
    std::os::unix::fs::symlink(paths.unit(&manifest.unit_version).cli(), paths.bin_link()).unwrap();

    fs::remove_file(paths.codex_skill_link()).unwrap();
    let error = installer.install(request()).await.unwrap_err();
    assert!(error.to_string().contains("is damaged"));
    std::os::unix::fs::symlink(
        paths.unit(&manifest.unit_version).skill(),
        paths.codex_skill_link(),
    )
    .unwrap();

    fs::write(paths.unit(&manifest.unit_version).stockfish(), b"damaged").unwrap();
    let error = installer.install(request()).await.unwrap_err();
    assert!(error.to_string().contains("is damaged"));
    assert_eq!(downloader.calls().len(), 1);
    fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn mutating_runtime_commands_reject_concurrent_ownership() {
    let root = test_root("lock");
    fs::remove_dir_all(&root).ok();
    fs::create_dir_all(&root).unwrap();
    let source_cli = root.join("chenchess-source");
    fs::write(&source_cli, b"CLI").unwrap();
    executable(&source_cli);
    let archive = stockfish_archive();
    let manifest = manifest(hex_sha256(&archive));
    let downloader =
        FakeDownloader::new(HashMap::from([(manifest.stockfish.url.clone(), archive)]));
    let runner = RecordingRunner::healthy();
    let paths = RuntimePaths::from_home(&root.join("home"));
    fs::create_dir_all(paths.state_dir()).unwrap();
    let lock = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(paths.state_dir().join("runtime.lock"))
        .unwrap();
    lock.try_lock_exclusive().unwrap();

    let error = RuntimeInstaller::new(downloader.clone(), runner.clone())
        .install(InstallRequest {
            manifest,
            paths,
            source_cli,
        })
        .await
        .expect_err("a second runtime mutation must not run concurrently");
    assert!(error.to_string().contains("another runtime mutation"));
    assert!(downloader.calls().is_empty());
    assert!(runner.calls().is_empty());
    lock.unlock().unwrap();
    fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn a_different_unit_updates_every_active_runtime_link_after_verification() {
    let root = test_root("upgrade");
    fs::remove_dir_all(&root).ok();
    fs::create_dir_all(&root).unwrap();
    let source_cli = root.join("chenchess-source");
    fs::write(&source_cli, b"CLI v1").unwrap();
    executable(&source_cli);
    let archive = stockfish_archive();
    let first_manifest = manifest(hex_sha256(&archive));
    let mut second_manifest = first_manifest.clone();
    second_manifest.unit_version = "0.2.0-test".to_string();
    let downloader = FakeDownloader::new(HashMap::from([(
        first_manifest.stockfish.url.clone(),
        archive,
    )]));
    let runner = StatefulRunner::new();
    let paths = RuntimePaths::from_home(&root.join("home"));
    let installer = RuntimeInstaller::new(downloader.clone(), runner.clone());

    installer
        .install(InstallRequest {
            manifest: first_manifest.clone(),
            paths: paths.clone(),
            source_cli: source_cli.clone(),
        })
        .await
        .unwrap();
    fs::write(&source_cli, b"CLI v2").unwrap();
    let outcome = installer
        .install(InstallRequest {
            manifest: second_manifest.clone(),
            paths: paths.clone(),
            source_cli,
        })
        .await
        .expect("a complete second unit should update atomically");
    assert_eq!(outcome, InstallOutcome::Updated);
    assert_eq!(
        fs::read_link(paths.bin_link()).unwrap(),
        paths.unit(&second_manifest.unit_version).cli()
    );
    assert_eq!(
        fs::read_link(paths.codex_skill_link()).unwrap(),
        paths.unit(&second_manifest.unit_version).skill()
    );
    let active: serde_json::Value =
        serde_json::from_slice(&fs::read(paths.config_file()).unwrap()).unwrap();
    assert_eq!(active["manifest"]["unitVersion"], "0.2.0-test");
    assert!(paths.unit(&first_manifest.unit_version).cli().is_file());
    RuntimeManager::new(runner)
        .uninstall(&paths)
        .expect("uninstall after update should remove every owned unit");
    assert!(!paths.unit(&first_manifest.unit_version).root().exists());
    assert!(!paths.unit(&second_manifest.unit_version).root().exists());
    fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn failed_update_keeps_the_previous_unit_active_and_healthy() {
    let root = test_root("failed-update");
    fs::remove_dir_all(&root).ok();
    fs::create_dir_all(&root).unwrap();
    let source_cli = root.join("chenchess-source");
    fs::write(&source_cli, b"CLI v1").unwrap();
    executable(&source_cli);
    let archive = stockfish_archive();
    let first_manifest = manifest(hex_sha256(&archive));
    let mut second_manifest = first_manifest.clone();
    second_manifest.unit_version = "0.2.0-test".to_string();
    let downloader = FakeDownloader::new(HashMap::from([(
        first_manifest.stockfish.url.clone(),
        archive,
    )]));
    let runner = StatefulRunner::new();
    let paths = RuntimePaths::from_home(&root.join("home"));
    let installer = RuntimeInstaller::new(downloader, runner.clone());
    installer
        .install(InstallRequest {
            manifest: first_manifest.clone(),
            paths: paths.clone(),
            source_cli: source_cli.clone(),
        })
        .await
        .unwrap();
    fs::write(&source_cli, b"CLI v2").unwrap();
    runner.fail_next_smoke();

    let error = installer
        .install(InstallRequest {
            manifest: second_manifest,
            paths: paths.clone(),
            source_cli,
        })
        .await
        .expect_err("a failed candidate smoke must roll back");

    assert!(error
        .to_string()
        .contains("live Review Session smoke failed"));
    assert_eq!(
        fs::read_link(paths.bin_link()).unwrap(),
        paths.unit(&first_manifest.unit_version).cli()
    );
    let active: serde_json::Value =
        serde_json::from_slice(&fs::read(paths.config_file()).unwrap()).unwrap();
    assert_eq!(active["manifest"]["unitVersion"], "0.1.0-test");
    assert_eq!(
        RuntimeManager::new(runner)
            .maia_status(&paths)
            .unwrap()
            .state,
        MaiaServiceState::Running
    );
    fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn interrupted_activation_recovers_the_previous_unit_and_converges_on_retry() {
    let root = test_root("interrupted-update");
    fs::remove_dir_all(&root).ok();
    fs::create_dir_all(&root).unwrap();
    let source_cli = root.join("chenchess-source");
    fs::write(&source_cli, b"CLI v1").unwrap();
    executable(&source_cli);
    let archive = stockfish_archive();
    let first_manifest = manifest(hex_sha256(&archive));
    let mut second_manifest = first_manifest.clone();
    second_manifest.unit_version = "0.2.0-test".to_string();
    let downloader = FakeDownloader::new(HashMap::from([(
        first_manifest.stockfish.url.clone(),
        archive,
    )]));
    let runner = StatefulRunner::new();
    let paths = RuntimePaths::from_home(&root.join("home"));
    let installer = RuntimeInstaller::new(downloader, runner);
    installer
        .install(InstallRequest {
            manifest: first_manifest,
            paths: paths.clone(),
            source_cli: source_cli.clone(),
        })
        .await
        .unwrap();

    let cli_backup = paths
        .bin_link()
        .parent()
        .unwrap()
        .join(".chenchess.previous");
    fs::rename(paths.bin_link(), cli_backup).unwrap();
    std::os::unix::fs::symlink(
        paths.unit(&second_manifest.unit_version).cli(),
        paths.bin_link(),
    )
    .unwrap();
    fs::write(&source_cli, b"CLI v2").unwrap();

    let outcome = installer
        .install(InstallRequest {
            manifest: second_manifest.clone(),
            paths: paths.clone(),
            source_cli,
        })
        .await
        .expect("retry should recover and finish the update");

    assert_eq!(outcome, InstallOutcome::Updated);
    assert_eq!(
        fs::read_link(paths.bin_link()).unwrap(),
        paths.unit(&second_manifest.unit_version).cli()
    );
    assert!(!paths
        .config_file()
        .parent()
        .unwrap()
        .join(".runtime.json.previous")
        .exists());
    assert!(!paths
        .bin_link()
        .parent()
        .unwrap()
        .join(".chenchess.previous")
        .exists());
    fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn config_published_before_links_recovers_the_previous_unit_and_converges_on_retry() {
    let root = test_root("config-first-interrupted-update");
    fs::remove_dir_all(&root).ok();
    fs::create_dir_all(&root).unwrap();
    let source_cli = root.join("chenchess-source");
    fs::write(&source_cli, b"CLI v1").unwrap();
    executable(&source_cli);
    let archive = stockfish_archive();
    let first_manifest = manifest(hex_sha256(&archive));
    let mut second_manifest = first_manifest.clone();
    second_manifest.unit_version = "0.2.0-test".to_string();
    let downloader = FakeDownloader::new(HashMap::from([(
        first_manifest.stockfish.url.clone(),
        archive,
    )]));
    let runner = StatefulRunner::new();
    let paths = RuntimePaths::from_home(&root.join("home"));
    let installer = RuntimeInstaller::new(downloader, runner);
    installer
        .install(InstallRequest {
            manifest: first_manifest.clone(),
            paths: paths.clone(),
            source_cli: source_cli.clone(),
        })
        .await
        .unwrap();

    let config_backup = paths
        .config_file()
        .parent()
        .unwrap()
        .join(".runtime.json.previous");
    fs::rename(paths.config_file(), &config_backup).unwrap();
    let mut interrupted_config: serde_json::Value =
        serde_json::from_slice(&fs::read(&config_backup).unwrap()).unwrap();
    interrupted_config["manifest"]["unitVersion"] =
        serde_json::Value::String(second_manifest.unit_version.clone());
    fs::write(
        paths.config_file(),
        serde_json::to_vec_pretty(&interrupted_config).unwrap(),
    )
    .unwrap();
    assert_eq!(
        fs::read_link(paths.bin_link()).unwrap(),
        paths.unit(&first_manifest.unit_version).cli()
    );
    fs::write(&source_cli, b"CLI v2").unwrap();

    let outcome = installer
        .install(InstallRequest {
            manifest: second_manifest.clone(),
            paths: paths.clone(),
            source_cli,
        })
        .await
        .expect("retry should roll back the partial publish and finish the update");

    assert_eq!(outcome, InstallOutcome::Updated);
    assert_eq!(
        fs::read_link(paths.bin_link()).unwrap(),
        paths.unit(&second_manifest.unit_version).cli()
    );
    let active: serde_json::Value =
        serde_json::from_slice(&fs::read(paths.config_file()).unwrap()).unwrap();
    assert_eq!(active["manifest"]["unitVersion"], "0.2.0-test");
    assert!(!config_backup.exists());
    fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn corrupt_installed_state_is_not_treated_as_a_fresh_install() {
    let root = test_root("corrupt-state");
    fs::remove_dir_all(&root).ok();
    fs::create_dir_all(&root).unwrap();
    let source_cli = root.join("chenchess-source");
    fs::write(&source_cli, b"CLI").unwrap();
    let archive = stockfish_archive();
    let manifest = manifest(hex_sha256(&archive));
    let downloader =
        FakeDownloader::new(HashMap::from([(manifest.stockfish.url.clone(), archive)]));
    let runner = RecordingRunner::healthy();
    let paths = RuntimePaths::from_home(&root.join("home"));
    fs::create_dir_all(paths.config_file().parent().unwrap()).unwrap();
    fs::write(paths.config_file(), b"not json").unwrap();

    let error = RuntimeInstaller::new(downloader.clone(), runner.clone())
        .install(InstallRequest {
            manifest,
            paths,
            source_cli,
        })
        .await
        .expect_err("corrupt installed state requires explicit repair");
    assert!(error.to_string().contains("expected ident"));
    assert!(downloader.calls().is_empty());
    assert!(runner.calls().is_empty());
    fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn runtime_metadata_reports_a_manifest_schema_mismatch() {
    let root = test_root("legacy-schema");
    fs::remove_dir_all(&root).ok();
    fs::create_dir_all(&root).unwrap();
    let source_cli = root.join("chenchess-source");
    fs::write(&source_cli, b"CLI").unwrap();
    executable(&source_cli);
    let archive = stockfish_archive();
    let manifest = manifest(hex_sha256(&archive));
    let downloader =
        FakeDownloader::new(HashMap::from([(manifest.stockfish.url.clone(), archive)]));
    let runner = StatefulRunner::new();
    let paths = RuntimePaths::from_home(&root.join("home"));
    RuntimeInstaller::new(downloader, runner.clone())
        .install(InstallRequest {
            manifest,
            paths: paths.clone(),
            source_cli,
        })
        .await
        .unwrap();
    let mut installed: serde_json::Value =
        serde_json::from_slice(&fs::read(paths.config_file()).unwrap()).unwrap();
    installed["manifest"]["schemaVersion"] = serde_json::json!(3);
    fs::write(paths.config_file(), serde_json::to_vec(&installed).unwrap()).unwrap();

    let error = RuntimeManager::new(runner)
        .doctor(
            &paths,
            Platform::new(std::env::consts::OS, std::env::consts::ARCH),
        )
        .expect_err("legacy metadata should require a full-unit update");

    assert!(error.to_string().contains("installed schema 3"));
    assert!(error.to_string().contains("supported schema 1"));
    fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn doctor_and_lifecycle_commands_verify_and_manage_only_the_installed_unit() {
    let root = test_root("doctor");
    fs::remove_dir_all(&root).ok();
    fs::create_dir_all(&root).unwrap();
    let source_cli = root.join("chenchess-source");
    fs::write(&source_cli, b"CLI").unwrap();
    executable(&source_cli);
    let archive = stockfish_archive();
    let manifest = manifest(hex_sha256(&archive));
    let downloader =
        FakeDownloader::new(HashMap::from([(manifest.stockfish.url.clone(), archive)]));
    let runner = StatefulRunner::new();
    let paths = RuntimePaths::from_home(&root.join("home"));
    RuntimeInstaller::new(downloader, runner.clone())
        .install(InstallRequest {
            manifest: manifest.clone(),
            paths: paths.clone(),
            source_cli,
        })
        .await
        .expect("runtime should install");
    let manager = RuntimeManager::new(runner.clone());
    let platform = Platform::current();

    let error = RuntimeManager::new(RecordingRunner::unavailable())
        .maia_status(&paths)
        .expect_err("status must not report stopped when Docker is unavailable");
    assert!(error.to_string().contains("docker failed"));

    let doctor = manager
        .doctor(&paths, platform.clone())
        .expect("installed runtime should be healthy");
    assert_eq!(doctor.unit_version, manifest.unit_version);
    assert_eq!(doctor.stockfish_version, "18");
    assert_eq!(doctor.maia_image, manifest.maia.image);
    assert_eq!(doctor.maia_package, manifest.maia.package);
    assert_eq!(doctor.maia_model, manifest.maia.model);
    assert_eq!(doctor.maia_state, MaiaServiceState::Running);
    let review = manager
        .review_config(&paths, platform.clone())
        .expect("review providers should resolve from the installed runtime");
    assert_eq!(
        review.config.stockfish_path,
        paths.unit(&manifest.unit_version).stockfish()
    );
    assert_eq!(review.config.maia_base_url, "http://127.0.0.1:38271");
    assert_eq!(review.config.unit_version, manifest.unit_version);
    let error = manager
        .stop_maia(&paths)
        .expect_err("an active review lease must block runtime mutation");
    assert!(error.to_string().contains("another runtime mutation"));
    drop(review);

    assert_eq!(
        manager.stop_maia(&paths).unwrap(),
        MaiaServiceState::Stopped
    );
    assert_eq!(
        manager.maia_status(&paths).unwrap().state,
        MaiaServiceState::Stopped
    );
    manager
        .review_config(&paths, platform.clone())
        .expect("review startup should auto-start a stopped Maia service");
    assert_eq!(
        manager.maia_status(&paths).unwrap().state,
        MaiaServiceState::Running
    );
    assert_eq!(
        manager.start_maia(&paths).unwrap(),
        MaiaServiceState::Running
    );
    let status = manager.maia_status(&paths).unwrap();
    assert_eq!(status.state, MaiaServiceState::Running);
    assert_eq!(status.unit_version, manifest.unit_version);
    assert_eq!(status.resources.as_deref(), Some("12MiB / 1GiB; 0.50%"));

    fs::write(paths.unit(&manifest.unit_version).stockfish(), b"damaged").unwrap();
    let error = manager
        .doctor(&paths, platform)
        .expect_err("doctor must detect damaged Stockfish");
    assert!(error.to_string().contains("checksum mismatch"));

    let calls = runner.calls();
    let installed: serde_json::Value =
        serde_json::from_slice(&fs::read(paths.config_file()).unwrap()).unwrap();
    let owned_name = format!(
        "chenchess-maia-{}-{}",
        manifest.unit_version,
        installed["ownerId"].as_str().unwrap()
    );
    assert!(calls.iter().any(|call| {
        call.program == Path::new("docker") && call.args == ["rm", "--force", &owned_name]
    }));
    assert!(!calls
        .iter()
        .any(|call| call.args.iter().any(|arg| arg == "maia")));
    fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn installation_preserves_unrelated_binary_and_same_named_container() {
    let root = test_root("ownership");
    fs::remove_dir_all(&root).ok();
    fs::create_dir_all(&root).unwrap();
    let source_cli = root.join("chenchess-source");
    fs::write(&source_cli, b"CLI").unwrap();
    executable(&source_cli);
    let archive = stockfish_archive();
    let base_manifest = manifest(hex_sha256(&archive));
    let paths = RuntimePaths::from_home(&root.join("home"));
    fs::create_dir_all(paths.bin_link().parent().unwrap()).unwrap();
    fs::write(paths.bin_link(), b"unrelated CLI").unwrap();
    let downloader = FakeDownloader::new(HashMap::from([(
        base_manifest.stockfish.url.clone(),
        archive.clone(),
    )]));
    let error = RuntimeInstaller::new(downloader, RecordingRunner::healthy())
        .install(InstallRequest {
            manifest: base_manifest.clone(),
            paths: paths.clone(),
            source_cli: source_cli.clone(),
        })
        .await
        .expect_err("unrelated binary must not be replaced");
    assert!(error.to_string().contains("path conflict"));
    assert_eq!(fs::read(paths.bin_link()).unwrap(), b"unrelated CLI");

    fs::remove_file(paths.bin_link()).unwrap();
    let runner = ForeignContainerRunner::new();
    let downloader = FakeDownloader::new(HashMap::from([(
        base_manifest.stockfish.url.clone(),
        archive,
    )]));
    let error = RuntimeInstaller::new(downloader, runner.clone())
        .install(InstallRequest {
            manifest: base_manifest,
            paths,
            source_cli,
        })
        .await
        .expect_err("foreign same-named container must not be removed");
    assert!(error.to_string().contains("container name conflict"));
    assert!(!runner
        .calls()
        .iter()
        .any(|args| args.first().is_some_and(|arg| arg == "rm")));

    let paths = RuntimePaths::from_home(&root.join("lifecycle-home"));
    let lifecycle_archive = stockfish_archive();
    let lifecycle_manifest = manifest(hex_sha256(&lifecycle_archive));
    let downloader = FakeDownloader::new(HashMap::from([(
        lifecycle_manifest.stockfish.url.clone(),
        lifecycle_archive,
    )]));
    RuntimeInstaller::new(downloader, RecordingRunner::healthy())
        .install(InstallRequest {
            manifest: lifecycle_manifest,
            paths: paths.clone(),
            source_cli: root.join("chenchess-source"),
        })
        .await
        .unwrap();
    let foreign = ForeignContainerRunner::new();
    let manager = RuntimeManager::new(foreign.clone());
    for error in [
        manager.start_maia(&paths).unwrap_err(),
        manager.stop_maia(&paths).unwrap_err(),
        manager.maia_status(&paths).unwrap_err(),
    ] {
        assert!(error.to_string().contains("container name conflict"));
    }
    assert!(!foreign
        .calls()
        .iter()
        .any(|args| args.first().is_some_and(|arg| arg == "rm")));
    fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn installation_preserves_unrelated_skill_discovery_entries() {
    let root = test_root("skill-ownership");
    fs::remove_dir_all(&root).ok();
    fs::create_dir_all(&root).unwrap();
    let source_cli = root.join("chenchess-source");
    fs::write(&source_cli, b"CLI").unwrap();
    executable(&source_cli);
    let archive = stockfish_archive();
    let manifest = manifest(hex_sha256(&archive));
    let paths = RuntimePaths::from_home(&root.join("home"));
    fs::create_dir_all(paths.codex_skill_link().parent().unwrap()).unwrap();
    fs::write(paths.codex_skill_link(), b"unrelated skill").unwrap();
    let downloader =
        FakeDownloader::new(HashMap::from([(manifest.stockfish.url.clone(), archive)]));
    let runner = RecordingRunner::healthy();

    let error = RuntimeInstaller::new(downloader.clone(), runner.clone())
        .install(InstallRequest {
            manifest,
            paths: paths.clone(),
            source_cli,
        })
        .await
        .expect_err("an unrelated Codex skill must not be replaced");

    assert!(error.to_string().contains("path conflict"));
    assert_eq!(
        fs::read(paths.codex_skill_link()).unwrap(),
        b"unrelated skill"
    );
    assert!(downloader.calls().is_empty());
    assert!(runner.calls().is_empty());
    fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn uninstall_removes_only_the_owned_runtime_and_skill_discovery_links() {
    let root = test_root("uninstall");
    fs::remove_dir_all(&root).ok();
    fs::create_dir_all(&root).unwrap();
    let source_cli = root.join("chenchess-source");
    fs::write(&source_cli, b"CLI").unwrap();
    executable(&source_cli);
    let archive = stockfish_archive();
    let manifest = manifest(hex_sha256(&archive));
    let downloader =
        FakeDownloader::new(HashMap::from([(manifest.stockfish.url.clone(), archive)]));
    let runner = StatefulRunner::new();
    let paths = RuntimePaths::from_home(&root.join("home"));
    RuntimeInstaller::new(downloader, runner.clone())
        .install(InstallRequest {
            manifest: manifest.clone(),
            paths: paths.clone(),
            source_cli,
        })
        .await
        .unwrap();
    let unrelated = root.join("home/.local/share/unrelated.txt");
    fs::write(&unrelated, b"keep me").unwrap();
    fs::write(paths.unit(&manifest.unit_version).stockfish(), b"damaged").unwrap();
    fs::remove_file(paths.codex_skill_link()).unwrap();

    RuntimeManager::new(runner.clone())
        .uninstall(&paths)
        .expect("owned runtime should uninstall");

    assert!(!paths.bin_link().exists());
    assert!(!paths.codex_skill_link().exists());
    assert!(!paths.claude_skill_link().exists());
    assert!(!paths.config_file().exists());
    assert!(!paths.unit(&manifest.unit_version).cli().exists());
    assert_eq!(fs::read(unrelated).unwrap(), b"keep me");
    assert!(runner.calls().iter().any(|call| {
        call.args.first().is_some_and(|arg| arg == "volume")
            && call.args.get(1).is_some_and(|arg| arg == "rm")
    }));
    fs::remove_dir_all(root).ok();
}

fn manifest(stockfish_sha256: String) -> RuntimeManifest {
    serde_json::from_value(serde_json::json!({
        "unitVersion": "0.1.0-test",
        "schemaVersion": 1,
        "target": { "os": std::env::consts::OS, "arch": std::env::consts::ARCH },
        "stockfish": {
            "version": "18",
            "url": "https://fixtures.test/stockfish.tar",
            "archiveSha256": stockfish_sha256,
            "binaryMember": "stockfish/stockfish-macos-m1-apple-silicon",
            "licenseMembers": ["stockfish/Copying.txt"]
        },
        "maia": {
            "image": "maia-runtime@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "package": "maia2==0.11.0",
            "model": "rapid",
            "modelSha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "configSha256": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
            "port": 38271
        }
    }))
    .expect("test manifest should parse")
}

fn stockfish_archive() -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut archive = tar::Builder::new(&mut bytes);
        append_tar_file(
            &mut archive,
            "stockfish/stockfish-macos-m1-apple-silicon",
            b"fake stockfish",
            0o755,
        );
        append_tar_file(&mut archive, "stockfish/Copying.txt", b"GPL notice", 0o644);
        archive.finish().expect("test archive should finish");
    }
    bytes
}

fn append_tar_file(
    archive: &mut tar::Builder<&mut Vec<u8>>,
    path: &str,
    contents: &[u8],
    mode: u32,
) {
    let mut header = tar::Header::new_gnu();
    header.set_size(contents.len() as u64);
    header.set_mode(mode);
    header.set_cksum();
    archive
        .append_data(&mut header, path, contents)
        .expect("test archive member should append");
}

fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn executable(path: &Path) {
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

#[derive(Clone)]
struct FakeDownloader {
    responses: Arc<HashMap<String, Vec<u8>>>,
    calls: Arc<Mutex<Vec<String>>>,
}

impl FakeDownloader {
    fn new(responses: HashMap<String, Vec<u8>>) -> Self {
        Self {
            responses: Arc::new(responses),
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl DownloadClient for FakeDownloader {
    fn download<'a>(
        &'a self,
        url: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, String>> + Send + 'a>> {
        Box::pin(async move {
            self.calls.lock().unwrap().push(url.to_string());
            self.responses
                .get(url)
                .cloned()
                .ok_or_else(|| format!("unexpected download {url}"))
        })
    }
}

#[derive(Clone)]
struct RecordingRunner {
    calls: Arc<Mutex<Vec<Vec<String>>>>,
    available: bool,
}

impl RecordingRunner {
    fn healthy() -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            available: true,
        }
    }

    fn unavailable() -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            available: false,
        }
    }

    fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().clone()
    }
}

impl ProcessRunner for RecordingRunner {
    fn run(&self, program: &Path, args: &[String]) -> Result<ProcessOutput, String> {
        if program != Path::new("docker") {
            return Ok(ProcessOutput {
                success: true,
                stdout: "id name Fake Stockfish\nuciok\n".to_string(),
                stderr: String::new(),
            });
        }
        self.calls.lock().unwrap().push(args.to_vec());
        let label_lookup = args.first().is_some_and(|arg| arg == "inspect")
            && args.iter().any(|arg| arg.contains("io.chenchess.runtime"));
        let stdout = if args.iter().any(|arg| arg == "{{.State.Running}}") {
            "true"
        } else if args.iter().any(|arg| arg == "{{.State.Health.Status}}") {
            "healthy"
        } else {
            "ok"
        };
        Ok(ProcessOutput {
            success: self.available && !label_lookup,
            stdout: stdout.to_string(),
            stderr: if self.available && !label_lookup {
                String::new()
            } else {
                "not found".to_string()
            },
        })
    }

    fn run_with_input(
        &self,
        program: &Path,
        args: &[String],
        input: &str,
    ) -> Result<ProcessOutput, String> {
        if input == "uci\nquit\n" {
            self.run(program, args)
        } else {
            assert_eq!(program, Path::new("/usr/bin/env"));
            assert_review_session_smoke_input(input);
            Ok(smoke_output())
        }
    }
}

#[derive(Debug, Clone)]
struct ProcessCall {
    program: PathBuf,
    args: Vec<String>,
}

#[derive(Clone)]
struct StatefulRunner {
    state: Arc<Mutex<StatefulRunnerState>>,
}

struct StatefulRunnerState {
    containers: HashMap<String, String>,
    calls: Vec<ProcessCall>,
    fail_next_smoke: bool,
}

impl StatefulRunner {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(StatefulRunnerState {
                containers: HashMap::new(),
                calls: Vec::new(),
                fail_next_smoke: false,
            })),
        }
    }

    fn calls(&self) -> Vec<ProcessCall> {
        self.state.lock().unwrap().calls.clone()
    }

    fn fail_next_smoke(&self) {
        self.state.lock().unwrap().fail_next_smoke = true;
    }
}

impl ProcessRunner for StatefulRunner {
    fn run(&self, program: &Path, args: &[String]) -> Result<ProcessOutput, String> {
        let mut state = self.state.lock().unwrap();
        state.calls.push(ProcessCall {
            program: program.to_path_buf(),
            args: args.to_vec(),
        });
        if program != Path::new("docker") {
            return Ok(ProcessOutput {
                success: true,
                stdout: "id name Fake Stockfish\nuciok\n".to_string(),
                stderr: String::new(),
            });
        }
        let first = args.first().map(String::as_str);
        let mut port_conflict = false;
        if first == Some("rm") {
            if let Some(name) = args.last() {
                state.containers.remove(name);
            }
        } else if first == Some("run") && args.iter().any(|arg| arg == "--detach") {
            if state.containers.is_empty() {
                let name = option_value(args, "--name").unwrap().to_string();
                let owner = option_value(args, "--label")
                    .unwrap()
                    .strip_prefix("io.chenchess.runtime=")
                    .unwrap()
                    .to_string();
                state.containers.insert(name, owner);
            } else {
                port_conflict = true;
            }
        }
        let container = args.last().map(String::as_str).unwrap_or_default();
        let running = state.containers.contains_key(container);
        let (success, stdout) = match args {
            [command, ..] if command == "version" => (true, "27.0.0"),
            [command, flag, format, ..]
                if command == "inspect" && flag == "--format" && format == "{{.State.Running}}" =>
            {
                (running, if running { "true" } else { "" })
            }
            [command, flag, format, ..]
                if command == "inspect"
                    && flag == "--format"
                    && format == "{{.State.Health.Status}}" =>
            {
                (running, if running { "healthy" } else { "" })
            }
            [command, flag, format, ..]
                if command == "inspect"
                    && flag == "--format"
                    && format.contains("io.chenchess.runtime") =>
            {
                (
                    running,
                    state.containers.get(container).map_or("", String::as_str),
                )
            }
            [command, ..] if command == "stats" => (running, "12MiB / 1GiB; 0.50%"),
            [command, ..] if command == "run" && port_conflict => (false, "port conflict"),
            _ => (true, "ok"),
        };
        Ok(ProcessOutput {
            success,
            stdout: stdout.to_string(),
            stderr: if success {
                String::new()
            } else {
                "not running".to_string()
            },
        })
    }

    fn run_with_input(
        &self,
        program: &Path,
        args: &[String],
        input: &str,
    ) -> Result<ProcessOutput, String> {
        if input == "uci\nquit\n" {
            self.run(program, args)
        } else {
            assert_eq!(program, Path::new("/usr/bin/env"));
            assert_review_session_smoke_input(input);
            let mut state = self.state.lock().unwrap();
            if state.fail_next_smoke {
                state.fail_next_smoke = false;
                return Ok(ProcessOutput {
                    success: false,
                    stdout: String::new(),
                    stderr: "candidate smoke failure".to_string(),
                });
            }
            Ok(smoke_output())
        }
    }
}

fn option_value<'a>(args: &'a [String], option: &str) -> Option<&'a str> {
    args.iter()
        .position(|arg| arg == option)
        .and_then(|index| args.get(index + 1))
        .map(String::as_str)
}

fn smoke_output() -> ProcessOutput {
    let events: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../packages/coach-engine-sdk/fixtures/events.json"
    ))
    .unwrap();
    let completion = events
        .into_iter()
        .find(|envelope| {
            envelope["event"]["kind"] == "completed"
                && envelope["event"]["result"]["kind"] == "gameImported"
        })
        .unwrap();
    ProcessOutput {
        success: true,
        stdout: format!("{completion}\n"),
        stderr: String::new(),
    }
}

fn assert_review_session_smoke_input(input: &str) {
    let command: serde_json::Value = serde_json::from_str(input).unwrap();
    assert_eq!(command["surface"], "coachSkill");
    assert_eq!(command["command"]["kind"], "importGame");
    assert_eq!(command["command"]["source"]["kind"], "pastedPgn");
    assert!(command["command"]["source"]["pgn"]
        .as_str()
        .unwrap()
        .contains("[Result \"0-1\"]"));
}

#[derive(Clone)]
struct ForeignContainerRunner {
    calls: Arc<Mutex<Vec<Vec<String>>>>,
}

impl ForeignContainerRunner {
    fn new() -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().clone()
    }
}

impl ProcessRunner for ForeignContainerRunner {
    fn run(&self, program: &Path, args: &[String]) -> Result<ProcessOutput, String> {
        if program != Path::new("docker") {
            return Ok(ProcessOutput {
                success: true,
                stdout: "id name Fake Stockfish\nuciok\n".to_string(),
                stderr: String::new(),
            });
        }
        self.calls.lock().unwrap().push(args.to_vec());
        let label_lookup = args.first().is_some_and(|arg| arg == "inspect")
            && args.iter().any(|arg| arg.contains("io.chenchess.runtime"));
        let health_lookup = args.iter().any(|arg| arg == "{{.State.Health.Status}}");
        Ok(ProcessOutput {
            success: true,
            stdout: if label_lookup {
                "foreign"
            } else if health_lookup {
                "healthy"
            } else {
                "ok"
            }
            .to_string(),
            stderr: String::new(),
        })
    }

    fn run_with_input(
        &self,
        program: &Path,
        args: &[String],
        input: &str,
    ) -> Result<ProcessOutput, String> {
        assert_eq!(input, "uci\nquit\n");
        self.run(program, args)
    }
}

fn test_root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("chenchess-runtime-{name}-{}", std::process::id()))
}
