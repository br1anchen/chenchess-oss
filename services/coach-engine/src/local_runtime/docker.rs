use std::path::Path;

use super::{
    run_checked, run_checked_owned, InstalledRuntime, ProcessRunner, RuntimeError, RuntimeManifest,
};

pub(super) fn model_volume(version: &str, owner_id: &str) -> String {
    format!("chenchess-maia-model-{version}-{owner_id}")
}

pub(super) fn maia_container(version: &str, owner_id: &str) -> String {
    format!("chenchess-maia-{version}-{owner_id}")
}

pub(super) fn start_container<R: ProcessRunner>(
    runner: &R,
    manifest: &RuntimeManifest,
    owner_id: &str,
    volume: &str,
    container_name: &str,
) -> Result<(), RuntimeError> {
    remove_owned_container(runner, container_name, owner_id)?;
    let args = vec![
        "run".to_string(),
        "--detach".to_string(),
        "--pull=never".to_string(),
        "--name".to_string(),
        container_name.to_string(),
        "--label".to_string(),
        format!("io.chenchess.runtime={owner_id}"),
        "--publish".to_string(),
        format!("127.0.0.1:{}:8080", manifest.maia.port),
        "--volume".to_string(),
        format!("{volume}:/models"),
        "--env".to_string(),
        "MAIA_MODEL_DIR=/models".to_string(),
        "--env".to_string(),
        format!("MAIA_MODEL_TYPE={}", manifest.maia.model),
        manifest.maia.image.clone(),
    ];
    run_checked_owned(runner, Path::new("docker"), args)?;
    if let Err(error) = wait_for_maia_health(runner, container_name) {
        let _ = remove_owned_container(runner, container_name, owner_id);
        return Err(error);
    }
    Ok(())
}

fn wait_for_maia_health<R: ProcessRunner>(
    runner: &R,
    container_name: &str,
) -> Result<(), RuntimeError> {
    for _ in 0..240 {
        match docker_inspect(runner, "{{.State.Health.Status}}", container_name) {
            Ok(health) if health.trim() == "healthy" => return Ok(()),
            Ok(_) => std::thread::sleep(std::time::Duration::from_secs(2)),
            Err(error) => return Err(error),
        }
    }
    Err(RuntimeError::Unhealthy(
        "Maia service did not become healthy within 8 minutes".to_string(),
    ))
}

pub(super) fn owned_container_running<R: ProcessRunner>(
    runner: &R,
    container_name: &str,
    owner_id: &str,
) -> Result<bool, RuntimeError> {
    if !container_owned_by(runner, container_name, owner_id)? {
        return Ok(false);
    }
    Ok(docker_inspect(runner, "{{.State.Running}}", container_name)?.trim() == "true")
}

pub(super) fn remove_owned_container<R: ProcessRunner>(
    runner: &R,
    container_name: &str,
    owner_id: &str,
) -> Result<(), RuntimeError> {
    if container_owned_by(runner, container_name, owner_id)? {
        run_checked(
            runner,
            Path::new("docker"),
            &["rm", "--force", container_name],
        )?;
    }
    Ok(())
}

pub(super) fn cleanup_container<R: ProcessRunner>(
    runner: &R,
    replacement: &InstalledRuntime,
) -> Result<(), RuntimeError> {
    remove_owned_container(runner, &replacement.container_name(), &replacement.owner_id)
}

fn container_owned_by<R: ProcessRunner>(
    runner: &R,
    container_name: &str,
    owner_id: &str,
) -> Result<bool, RuntimeError> {
    let label_format = "{{ index .Config.Labels \"io.chenchess.runtime\" }}";
    let lookup = runner
        .run(
            Path::new("docker"),
            &[
                "inspect".to_string(),
                "--format".to_string(),
                label_format.to_string(),
                container_name.to_string(),
            ],
        )
        .map_err(|message| RuntimeError::Process {
            program: "docker".to_string(),
            message,
        })?;
    if !lookup.success {
        return Ok(false);
    }
    let owner = lookup.stdout.trim();
    if owner != owner_id {
        return Err(RuntimeError::ContainerNameConflict {
            name: container_name.to_string(),
            owner: owner.to_string(),
        });
    }
    Ok(true)
}

pub(super) fn docker_inspect<R: ProcessRunner>(
    runner: &R,
    format: &str,
    container_name: &str,
) -> Result<String, RuntimeError> {
    Ok(run_checked(
        runner,
        Path::new("docker"),
        &["inspect", "--format", format, container_name],
    )?
    .stdout)
}
