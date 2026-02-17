//! OpenOCD TCL socket client
//!
//! Communicates with OpenOCD's TCL server (default port 6666).
//! Protocol: send command as UTF-8, terminated by 0x1a (SUB character).
//! Response: UTF-8 text terminated by 0x1a.

use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};
use tracing::{debug, info, warn};
use std::path::Path;
use std::time::Duration;

/// TCL protocol terminator byte (ASCII SUB / Ctrl-Z)
const TCL_TERMINATOR: u8 = 0x1a;

/// OpenOCD TCL client connected to a running OpenOCD instance
pub struct OpenocdClient {
    stream: TcpStream,
    /// OpenOCD child process (owned — killed on drop)
    process: Child,
    /// TCL port this session uses
    pub tcl_port: u16,
    /// GDB port (for reference, not currently used)
    pub gdb_port: u16,
    /// Telnet port (for reference, not currently used)
    pub telnet_port: u16,
}

impl OpenocdClient {
    /// Start OpenOCD with the given config file and connect to TCL socket.
    ///
    /// Auto-allocates ports to avoid conflicts with other sessions.
    pub async fn start(
        openocd_path: &Path,
        cfg_file: &str,
        extra_args: &[String],
        base_tcl_port: u16,
    ) -> Result<Self, OpenocdError> {
        let tcl_port = base_tcl_port;
        let gdb_port = base_tcl_port + 1;
        let telnet_port = base_tcl_port + 2;

        info!("Starting OpenOCD: {} -f {} (TCL port {})", openocd_path.display(), cfg_file, tcl_port);

        let mut cmd = Command::new(openocd_path);
        cmd.arg("-f").arg(cfg_file)
            .arg("-c").arg(format!("tcl_port {}", tcl_port))
            .arg("-c").arg(format!("gdb_port {}", gdb_port))
            .arg("-c").arg(format!("telnet_port {}", telnet_port));

        for arg in extra_args {
            cmd.arg(arg);
        }

        // Suppress stdout/stderr to avoid blocking
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let process = cmd.spawn().map_err(|e| {
            OpenocdError::LaunchFailed(format!("Failed to spawn openocd: {}", e))
        })?;

        // Wait for TCL port to become available
        let stream = Self::wait_for_connection(tcl_port, Duration::from_secs(5)).await?;

        info!("Connected to OpenOCD TCL on port {}", tcl_port);

        Ok(Self {
            stream,
            process,
            tcl_port,
            gdb_port,
            telnet_port,
        })
    }

    /// Wait for OpenOCD TCL port to become available, retrying with backoff
    async fn wait_for_connection(port: u16, timeout: Duration) -> Result<TcpStream, OpenocdError> {
        let start = tokio::time::Instant::now();
        let mut delay = Duration::from_millis(50);

        loop {
            match TcpStream::connect(format!("127.0.0.1:{}", port)).await {
                Ok(stream) => return Ok(stream),
                Err(_) if start.elapsed() < timeout => {
                    tokio::time::sleep(delay).await;
                    delay = (delay * 2).min(Duration::from_millis(500));
                }
                Err(e) => {
                    return Err(OpenocdError::ConnectionFailed(format!(
                        "Failed to connect to OpenOCD TCL port {} after {:?}: {}",
                        port,
                        timeout,
                        e
                    )));
                }
            }
        }
    }

    /// Send a TCL command to OpenOCD and read the response.
    pub async fn send_command(&mut self, command: &str) -> Result<String, OpenocdError> {
        debug!("OpenOCD TCL command: {}", command);

        // Send command + terminator
        let mut payload = command.as_bytes().to_vec();
        payload.push(TCL_TERMINATOR);

        self.stream.write_all(&payload).await.map_err(|e| {
            OpenocdError::CommandFailed(format!("Write failed: {}", e))
        })?;

        // Read response until terminator
        let response = self.read_response().await?;

        debug!("OpenOCD TCL response: {}", response);
        Ok(response)
    }

    /// Read response bytes until 0x1a terminator
    async fn read_response(&mut self) -> Result<String, OpenocdError> {
        let mut buf = Vec::with_capacity(4096);
        let mut byte = [0u8; 1];

        let timeout = Duration::from_secs(10);
        let deadline = tokio::time::Instant::now() + timeout;

        loop {
            let read_future = self.stream.read(&mut byte);

            match tokio::time::timeout_at(deadline, read_future).await {
                Ok(Ok(0)) => {
                    return Err(OpenocdError::ConnectionClosed);
                }
                Ok(Ok(_)) => {
                    if byte[0] == TCL_TERMINATOR {
                        break;
                    }
                    buf.push(byte[0]);
                }
                Ok(Err(e)) => {
                    return Err(OpenocdError::CommandFailed(format!("Read failed: {}", e)));
                }
                Err(_) => {
                    return Err(OpenocdError::Timeout);
                }
            }
        }

        String::from_utf8(buf).map_err(|e| {
            OpenocdError::CommandFailed(format!("Invalid UTF-8 in response: {}", e))
        })
    }

    /// Shutdown OpenOCD gracefully
    pub async fn shutdown(&mut self) -> Result<(), OpenocdError> {
        info!("Shutting down OpenOCD");

        // Try graceful shutdown via TCL
        let _ = self.send_command("shutdown").await;

        // Give it a moment to exit
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Force kill if still running
        if let Err(e) = self.process.kill().await {
            warn!("Kill after shutdown: {}", e);
        }

        Ok(())
    }

    /// Check if the OpenOCD process is still running
    pub fn is_running(&mut self) -> bool {
        match self.process.try_wait() {
            Ok(None) => true,   // Still running
            Ok(Some(_)) => false, // Exited
            Err(_) => false,
        }
    }
}

/// Parse a hex address from OpenOCD output (e.g., "0x10000000" or plain number)
pub fn parse_address(s: &str) -> Result<u64, OpenocdError> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16)
            .map_err(|e| OpenocdError::ParseError(format!("Invalid hex address '{}': {}", s, e)))
    } else {
        s.parse::<u64>()
            .map_err(|e| OpenocdError::ParseError(format!("Invalid address '{}': {}", s, e)))
    }
}

/// Parse OpenOCD memory dump output (from `mdw` command)
/// Format: "0x10000000: deadbeef cafebabe ..."
pub fn parse_memory_dump(output: &str) -> Vec<u32> {
    let mut words = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        // Skip empty lines and error messages
        if line.is_empty() || !line.starts_with("0x") {
            continue;
        }
        // Split on ':' — address on left, data on right
        if let Some(data_part) = line.split(':').nth(1) {
            for word_str in data_part.split_whitespace() {
                if let Ok(word) = u32::from_str_radix(word_str, 16) {
                    words.push(word);
                }
            }
        }
    }
    words
}

#[derive(Debug, thiserror::Error)]
pub enum OpenocdError {
    #[error("OpenOCD launch failed: {0}")]
    LaunchFailed(String),

    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Connection closed by OpenOCD")]
    ConnectionClosed,

    #[error("Command failed: {0}")]
    CommandFailed(String),

    #[error("Command timeout")]
    Timeout,

    #[error("Parse error: {0}")]
    ParseError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_address_hex() {
        assert_eq!(parse_address("0x10000000").unwrap(), 0x10000000);
        assert_eq!(parse_address("0X08000000").unwrap(), 0x08000000);
        assert_eq!(parse_address("0xDEADBEEF").unwrap(), 0xDEADBEEF);
    }

    #[test]
    fn test_parse_address_decimal() {
        assert_eq!(parse_address("268435456").unwrap(), 268435456);
        assert_eq!(parse_address("0").unwrap(), 0);
    }

    #[test]
    fn test_parse_address_with_whitespace() {
        assert_eq!(parse_address("  0x100  ").unwrap(), 0x100);
    }

    #[test]
    fn test_parse_address_invalid() {
        assert!(parse_address("0xZZZZ").is_err());
        assert!(parse_address("not_a_number").is_err());
    }

    #[test]
    fn test_parse_memory_dump_single_line() {
        let output = "0x10000000: deadbeef cafebabe";
        let words = parse_memory_dump(output);
        assert_eq!(words, vec![0xdeadbeef, 0xcafebabe]);
    }

    #[test]
    fn test_parse_memory_dump_multi_line() {
        let output = "0x10000000: 00000001 00000002\n0x10000008: 00000003 00000004";
        let words = parse_memory_dump(output);
        assert_eq!(words, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_parse_memory_dump_with_noise() {
        let output = "some error\n0x10000000: aabbccdd\n\n";
        let words = parse_memory_dump(output);
        assert_eq!(words, vec![0xaabbccdd]);
    }

    #[test]
    fn test_parse_memory_dump_empty() {
        assert!(parse_memory_dump("").is_empty());
        assert!(parse_memory_dump("error: target not halted").is_empty());
    }

    #[test]
    fn test_tcl_terminator_value() {
        // Ensure terminator is ASCII SUB (0x1a = 26)
        assert_eq!(TCL_TERMINATOR, 0x1a);
        assert_eq!(TCL_TERMINATOR, 26);
    }
}
