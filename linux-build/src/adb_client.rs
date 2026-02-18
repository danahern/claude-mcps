//! ADB CLI wrapper for Android Debug Bridge operations
//!
//! Parallel to docker_client.rs SSH functions, wraps `adb` CLI for shell,
//! file transfer, and device discovery.

use tokio::process::Command;
use tracing::{debug, info};

use crate::docker_client::ExecResult;

/// ADB device info from `adb devices`
#[derive(Debug, Clone)]
pub struct AdbDevice {
    pub serial: String,
    pub state: String,
}

/// Run a shell command on the device via ADB
pub async fn adb_shell(
    command: &str,
    serial: Option<&str>,
) -> Result<ExecResult, AdbError> {
    info!("adb shell: {}", command);

    let mut cmd = Command::new("adb");
    if let Some(s) = serial {
        cmd.arg("-s").arg(s);
    }
    cmd.arg("shell").arg(command);

    let output = cmd.output().await.map_err(|e| {
        AdbError::CommandFailed(format!("Failed to run adb: {}", e))
    })?;

    Ok(ExecResult {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

/// Push a file to the device via ADB
pub async fn adb_push(
    file_path: &str,
    remote_path: &str,
    serial: Option<&str>,
) -> Result<String, AdbError> {
    info!("adb push {} -> {}", file_path, remote_path);

    let mut cmd = Command::new("adb");
    if let Some(s) = serial {
        cmd.arg("-s").arg(s);
    }
    cmd.arg("push").arg(file_path).arg(remote_path);

    let output = cmd.output().await.map_err(|e| {
        AdbError::PushFailed(format!("Failed to run adb push: {}", e))
    })?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(AdbError::PushFailed(stderr.trim().to_string()))
    }
}

/// Pull a file from the device via ADB
pub async fn adb_pull(
    remote_path: &str,
    local_path: &str,
    serial: Option<&str>,
) -> Result<String, AdbError> {
    info!("adb pull {} -> {}", remote_path, local_path);

    let mut cmd = Command::new("adb");
    if let Some(s) = serial {
        cmd.arg("-s").arg(s);
    }
    cmd.arg("pull").arg(remote_path).arg(local_path);

    let output = cmd.output().await.map_err(|e| {
        AdbError::PullFailed(format!("Failed to run adb pull: {}", e))
    })?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(AdbError::PullFailed(stderr.trim().to_string()))
    }
}

/// List connected ADB devices
pub async fn adb_devices() -> Result<Vec<AdbDevice>, AdbError> {
    debug!("adb devices");

    let output = Command::new("adb")
        .arg("devices")
        .output()
        .await
        .map_err(|e| AdbError::CommandFailed(format!("Failed to run adb: {}", e)))?;

    if !output.status.success() {
        return Err(AdbError::CommandFailed(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let devices: Vec<AdbDevice> = stdout
        .lines()
        .skip(1) // skip "List of devices attached" header
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                Some(AdbDevice {
                    serial: parts[0].to_string(),
                    state: parts[1].to_string(),
                })
            } else {
                None
            }
        })
        .collect();

    Ok(devices)
}

/// Flash a compressed WIC image to a device via ADB
///
/// Pipes `bzcat <image>` into `adb shell dd of=<device> bs=4M`
pub async fn flash_image_adb(
    image_path: &str,
    device: &str,
    serial: Option<&str>,
) -> Result<String, AdbError> {
    info!("Flashing {} to {} via ADB", image_path, device);

    // Use std::process for piping bzcat into adb — Tokio's ChildStdout
    // doesn't impl Into<Stdio>, so we use blocking spawn for the pipeline.
    let output = tokio::task::spawn_blocking({
        let image_path = image_path.to_string();
        let device = device.to_string();
        let serial = serial.map(|s| s.to_string());
        move || {
            let bzcat = std::process::Command::new("bzcat")
                .arg(&image_path)
                .stdout(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| AdbError::CommandFailed(format!("Failed to start bzcat: {}", e)))?;

            let bzcat_stdout = bzcat.stdout.expect("bzcat stdout was piped");

            let mut adb_cmd = std::process::Command::new("adb");
            if let Some(s) = &serial {
                adb_cmd.arg("-s").arg(s);
            }
            adb_cmd.arg("shell")
                .arg(format!("dd of={} bs=4M", device))
                .stdin(bzcat_stdout);

            adb_cmd.output().map_err(|e| AdbError::CommandFailed(format!("ADB flash failed: {}", e)))
        }
    }).await.map_err(|e| AdbError::CommandFailed(format!("Task join failed: {}", e)))??;

    if output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Ok(stderr.trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(AdbError::CommandFailed(format!("Flash failed: {}", stderr.trim())))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AdbError {
    #[error("ADB command failed: {0}")]
    CommandFailed(String),

    #[error("ADB push failed: {0}")]
    PushFailed(String),

    #[error("ADB pull failed: {0}")]
    PullFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adb_error_display() {
        let err = AdbError::CommandFailed("test".to_string());
        assert!(err.to_string().contains("ADB command failed"));

        let err = AdbError::PushFailed("no device".to_string());
        assert!(err.to_string().contains("ADB push failed"));

        let err = AdbError::PullFailed("not found".to_string());
        assert!(err.to_string().contains("ADB pull failed"));
    }
}
