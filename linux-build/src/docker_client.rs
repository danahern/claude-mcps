//! Docker CLI wrapper for container lifecycle management
//!
//! Uses `docker` CLI (not Docker API) for simplicity.
//! Containers mount the host workspace for edit-on-Mac, build-in-container workflow.

use tokio::process::Command;
use tracing::{debug, info, warn};
use std::path::Path;

/// Result of a Docker command execution
#[derive(Debug)]
pub struct ExecResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Start a new Docker container with workspace mounted
pub async fn start_container(
    image: &str,
    container_name: &str,
    workspace_dir: Option<&Path>,
    extra_volumes: &[String],
) -> Result<String, DockerError> {
    info!("Starting container '{}' from image '{}'", container_name, image);

    let mut cmd = Command::new("docker");
    cmd.arg("run")
        .arg("-d")  // detached
        .arg("--name").arg(container_name);

    // Mount workspace if provided
    if let Some(ws) = workspace_dir {
        cmd.arg("-v").arg(format!("{}:/workspace", ws.display()));
    }

    // Mount extra volumes
    for vol in extra_volumes {
        cmd.arg("-v").arg(vol);
    }

    // Create /artifacts directory for build outputs
    cmd.arg(image)
        .arg("bash").arg("-c").arg("mkdir -p /artifacts && sleep infinity");

    let output = cmd.output().await.map_err(|e| {
        DockerError::CommandFailed(format!("Failed to run docker: {}", e))
    })?;

    if output.status.success() {
        let container_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        info!("Container started: {}", &container_id[..12.min(container_id.len())]);
        Ok(container_id)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        Err(DockerError::ContainerStartFailed(stderr))
    }
}

/// Stop and remove a container
pub async fn stop_container(container_name: &str) -> Result<(), DockerError> {
    info!("Stopping container '{}'", container_name);

    // Stop
    let output = Command::new("docker")
        .args(["stop", "-t", "5", container_name])
        .output().await
        .map_err(|e| DockerError::CommandFailed(format!("docker stop failed: {}", e)))?;

    if !output.status.success() {
        warn!("docker stop returned non-zero (container may already be stopped)");
    }

    // Remove
    let output = Command::new("docker")
        .args(["rm", "-f", container_name])
        .output().await
        .map_err(|e| DockerError::CommandFailed(format!("docker rm failed: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!("docker rm: {}", stderr.trim());
    }

    Ok(())
}

/// Check if a container is running
pub async fn container_status(container_name: &str) -> Result<ContainerState, DockerError> {
    let output = Command::new("docker")
        .args(["inspect", "--format", "{{.State.Status}}", container_name])
        .output().await
        .map_err(|e| DockerError::CommandFailed(format!("docker inspect failed: {}", e)))?;

    if output.status.success() {
        let status = String::from_utf8_lossy(&output.stdout).trim().to_string();
        match status.as_str() {
            "running" => Ok(ContainerState::Running),
            "exited" => Ok(ContainerState::Exited),
            "created" => Ok(ContainerState::Created),
            other => Ok(ContainerState::Other(other.to_string())),
        }
    } else {
        Ok(ContainerState::NotFound)
    }
}

/// Execute a command inside a running container
pub async fn exec_command(
    container_name: &str,
    command: &str,
    workdir: Option<&str>,
) -> Result<ExecResult, DockerError> {
    debug!("docker exec in '{}': {}", container_name, command);

    let mut cmd = Command::new("docker");
    cmd.arg("exec");

    if let Some(wd) = workdir {
        cmd.arg("-w").arg(wd);
    }

    cmd.arg(container_name)
        .arg("bash").arg("-c").arg(command);

    let output = cmd.output().await.map_err(|e| {
        DockerError::CommandFailed(format!("docker exec failed: {}", e))
    })?;

    let exit_code = output.status.code().unwrap_or(-1);

    Ok(ExecResult {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code,
    })
}

/// Copy files from container to host
pub async fn copy_from_container(
    container_name: &str,
    container_path: &str,
    host_path: &str,
) -> Result<(), DockerError> {
    let src = format!("{}:{}", container_name, container_path);

    let output = Command::new("docker")
        .args(["cp", &src, host_path])
        .output().await
        .map_err(|e| DockerError::CommandFailed(format!("docker cp failed: {}", e)))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(DockerError::CopyFailed(stderr.to_string()))
    }
}

/// Run SCP to deploy file to board
pub async fn scp_deploy(
    file_path: &str,
    user: &str,
    board_ip: &str,
    remote_path: &str,
    ssh_key: Option<&Path>,
) -> Result<(), DockerError> {
    info!("Deploying {} to {}@{}:{}", file_path, user, board_ip, remote_path);

    let mut cmd = Command::new("scp");
    cmd.arg("-o").arg("StrictHostKeyChecking=no");

    if let Some(key) = ssh_key {
        cmd.arg("-i").arg(key);
    }

    cmd.arg(file_path)
        .arg(format!("{}@{}:{}", user, board_ip, remote_path));

    let output = cmd.output().await.map_err(|e| {
        DockerError::CommandFailed(format!("scp failed: {}", e))
    })?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(DockerError::DeployFailed(stderr.to_string()))
    }
}

/// Run SSH command on board
pub async fn ssh_command(
    user: &str,
    board_ip: &str,
    command: &str,
    ssh_key: Option<&Path>,
) -> Result<ExecResult, DockerError> {
    info!("SSH {}@{}: {}", user, board_ip, command);

    let mut cmd = Command::new("ssh");
    cmd.arg("-o").arg("StrictHostKeyChecking=no")
        .arg("-o").arg("ConnectTimeout=5");

    if let Some(key) = ssh_key {
        cmd.arg("-i").arg(key);
    }

    cmd.arg(format!("{}@{}", user, board_ip))
        .arg(command);

    let output = cmd.output().await.map_err(|e| {
        DockerError::CommandFailed(format!("ssh failed: {}", e))
    })?;

    Ok(ExecResult {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

/// Flash a compressed WIC image to a board via SSH
///
/// Pipes `bzcat <image>` into `ssh <user>@<ip> dd of=<device> bs=4M`
pub async fn flash_image_ssh(
    image_path: &str,
    user: &str,
    board_ip: &str,
    device: &str,
    ssh_key: Option<&Path>,
) -> Result<String, DockerError> {
    info!("Flashing {} to {}@{}:{}", image_path, user, board_ip, device);

    // Use std::process for piping bzcat into ssh — Tokio's ChildStdout
    // doesn't impl Into<Stdio>, so we use blocking spawn for the pipeline.
    let output = tokio::task::spawn_blocking({
        let image_path = image_path.to_string();
        let user = user.to_string();
        let board_ip = board_ip.to_string();
        let device = device.to_string();
        let ssh_key = ssh_key.map(|p| p.to_path_buf());
        move || {
            let bzcat = std::process::Command::new("bzcat")
                .arg(&image_path)
                .stdout(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| DockerError::CommandFailed(format!("Failed to start bzcat: {}", e)))?;

            let bzcat_stdout = bzcat.stdout.expect("bzcat stdout was piped");

            let mut ssh_cmd = std::process::Command::new("ssh");
            ssh_cmd.arg("-o").arg("StrictHostKeyChecking=no")
                .arg("-o").arg("ConnectTimeout=10");
            if let Some(key) = &ssh_key {
                ssh_cmd.arg("-i").arg(key);
            }
            ssh_cmd.arg(format!("{}@{}", user, board_ip))
                .arg(format!("dd of={} bs=4M", device))
                .stdin(bzcat_stdout);

            ssh_cmd.output().map_err(|e| DockerError::CommandFailed(format!("SSH flash failed: {}", e)))
        }
    }).await.map_err(|e| DockerError::CommandFailed(format!("Task join failed: {}", e)))??;

    if output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Ok(stderr.trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(DockerError::CommandFailed(format!("Flash failed: {}", stderr.trim())))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ContainerState {
    Running,
    Exited,
    Created,
    NotFound,
    Other(String),
}

impl std::fmt::Display for ContainerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Running => write!(f, "running"),
            Self::Exited => write!(f, "exited"),
            Self::Created => write!(f, "created"),
            Self::NotFound => write!(f, "not found"),
            Self::Other(s) => write!(f, "{}", s),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DockerError {
    #[error("Docker command failed: {0}")]
    CommandFailed(String),

    #[error("Container start failed: {0}")]
    ContainerStartFailed(String),

    #[error("Copy failed: {0}")]
    CopyFailed(String),

    #[error("Deploy failed: {0}")]
    DeployFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_container_state_display() {
        assert_eq!(ContainerState::Running.to_string(), "running");
        assert_eq!(ContainerState::NotFound.to_string(), "not found");
        assert_eq!(ContainerState::Other("paused".to_string()).to_string(), "paused");
    }
}
