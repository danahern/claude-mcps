//! RMCP 0.3.2 implementation for OpenOCD debug MCP tools
//!
//! Provides 10 MVP tools for target debugging via OpenOCD's TCL interface.

use rmcp::{
    tool, tool_router, tool_handler, ServerHandler,
    handler::server::{router::tool::ToolRouter, tool::Parameters},
    model::*,
    ErrorData as McpError,
};
use tracing::{info, warn};
use std::future::Future;
use std::collections::HashMap;
use std::sync::Arc;
use std::path::Path;
use tokio::sync::RwLock;

use super::types::*;
use crate::config::Config;
use crate::openocd_client::{OpenocdClient, parse_address, parse_memory_dump};

/// Active OpenOCD session
struct OpenocdSession {
    #[allow(dead_code)]
    session_id: String,
    #[allow(dead_code)]
    cfg_file: String,
    client: OpenocdClient,
}

/// Port allocator — each session gets 3 consecutive ports (tcl, gdb, telnet)
struct PortAllocator {
    next_base: u16,
}

impl PortAllocator {
    fn new() -> Self {
        Self { next_base: 6666 }
    }

    fn allocate(&mut self) -> u16 {
        let base = self.next_base;
        self.next_base += 3;
        // Wrap around if we get too high
        if self.next_base > 60000 {
            self.next_base = 6666;
        }
        base
    }
}

/// OpenOCD debug tool handler
#[derive(Clone)]
pub struct OpenocdDebugToolHandler {
    #[allow(dead_code)]
    tool_router: ToolRouter<OpenocdDebugToolHandler>,
    config: Config,
    sessions: Arc<RwLock<HashMap<String, Arc<tokio::sync::Mutex<OpenocdSession>>>>>,
    port_allocator: Arc<tokio::sync::Mutex<PortAllocator>>,
}

impl OpenocdDebugToolHandler {
    pub fn new(config: Config) -> Self {
        Self {
            tool_router: Self::tool_router(),
            config,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            port_allocator: Arc::new(tokio::sync::Mutex::new(PortAllocator::new())),
        }
    }

    /// Get a session by ID, returning an MCP error if not found
    async fn get_session(&self, session_id: &str) -> Result<Arc<tokio::sync::Mutex<OpenocdSession>>, McpError> {
        let sessions = self.sessions.read().await;
        sessions.get(session_id).cloned().ok_or_else(|| {
            McpError::invalid_params(format!("Session not found: {}", session_id), None)
        })
    }

    /// Execute a TCL command on a session, returning formatted result
    async fn tcl_command(&self, session_id: &str, command: &str) -> Result<String, McpError> {
        let session = self.get_session(session_id).await?;
        let mut session = session.lock().await;
        session.client.send_command(command).await.map_err(|e| {
            McpError::internal_error(format!("OpenOCD command failed: {}", e), None)
        })
    }
}

fn make_error(msg: impl Into<String>) -> McpError {
    McpError::internal_error(msg.into(), None)
}

#[tool_router]
impl OpenocdDebugToolHandler {
    // =========================================================================
    // Session Management (2 tools)
    // =========================================================================

    #[tool(description = "Start OpenOCD daemon, establish TCL session. Returns session_id for use with other tools.")]
    async fn connect(&self, Parameters(args): Parameters<ConnectArgs>) -> Result<CallToolResult, McpError> {
        let openocd_path = self.config.find_openocd().map_err(|e| make_error(e))?;

        let base_port = {
            let mut allocator = self.port_allocator.lock().await;
            allocator.allocate()
        };

        let extra_args = args.extra_args.unwrap_or_default();

        info!("Connecting to target via OpenOCD: cfg={}, tcl_port={}", args.cfg_file, base_port);

        let client = OpenocdClient::start(
            &openocd_path,
            &args.cfg_file,
            &extra_args,
            base_port,
        ).await.map_err(|e| make_error(format!("OpenOCD start failed: {}", e)))?;

        let session_id = uuid::Uuid::new_v4().to_string();

        let session = OpenocdSession {
            session_id: session_id.clone(),
            cfg_file: args.cfg_file.clone(),
            client,
        };

        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id.clone(), Arc::new(tokio::sync::Mutex::new(session)));
        }

        let message = format!(
            "Connected to OpenOCD\n\
             Session ID: {}\n\
             Config: {}\n\
             TCL port: {}\n\
             GDB port: {}\n\
             Telnet port: {}",
            session_id,
            args.cfg_file,
            base_port,
            base_port + 1,
            base_port + 2,
        );

        info!("Session {} created", session_id);
        Ok(CallToolResult::success(vec![Content::text(message)]))
    }

    #[tool(description = "Stop OpenOCD daemon and release session")]
    async fn disconnect(&self, Parameters(args): Parameters<DisconnectArgs>) -> Result<CallToolResult, McpError> {
        let session = {
            let mut sessions = self.sessions.write().await;
            sessions.remove(&args.session_id)
        };

        match session {
            Some(session) => {
                let mut session = session.lock().await;
                if let Err(e) = session.client.shutdown().await {
                    warn!("Shutdown error (non-fatal): {}", e);
                }
                info!("Session {} disconnected", args.session_id);
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Session {} disconnected", args.session_id
                ))]))
            }
            None => {
                Err(McpError::invalid_params(
                    format!("Session not found: {}", args.session_id),
                    None,
                ))
            }
        }
    }

    // =========================================================================
    // Target Control (4 tools)
    // =========================================================================

    #[tool(description = "Get current status of the target CPU and debug session")]
    async fn get_status(&self, Parameters(args): Parameters<GetStatusArgs>) -> Result<CallToolResult, McpError> {
        // Get target state
        let targets_output = self.tcl_command(&args.session_id, "targets").await?;

        // Try to get PC if halted
        let pc_output = self.tcl_command(&args.session_id, "reg pc").await
            .unwrap_or_else(|_| "PC not available".to_string());

        let message = format!("Target status:\n{}\n\nPC:\n{}", targets_output.trim(), pc_output.trim());
        Ok(CallToolResult::success(vec![Content::text(message)]))
    }

    #[tool(description = "Halt target CPU execution")]
    async fn halt(&self, Parameters(args): Parameters<HaltArgs>) -> Result<CallToolResult, McpError> {
        let output = self.tcl_command(&args.session_id, "halt").await?;
        info!("Target halted");
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Target halted\n{}", output.trim()
        ))]))
    }

    #[tool(description = "Resume target CPU execution")]
    async fn run(&self, Parameters(args): Parameters<RunArgs>) -> Result<CallToolResult, McpError> {
        let output = self.tcl_command(&args.session_id, "resume").await?;
        info!("Target resumed");
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Target resumed\n{}", output.trim()
        ))]))
    }

    #[tool(description = "Reset the target CPU")]
    async fn reset(&self, Parameters(args): Parameters<ResetArgs>) -> Result<CallToolResult, McpError> {
        let cmd = if args.halt_after_reset {
            "reset halt"
        } else {
            "reset run"
        };
        let output = self.tcl_command(&args.session_id, cmd).await?;
        info!("Target reset ({})", cmd);
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Target reset ({})\n{}", cmd, output.trim()
        ))]))
    }

    // =========================================================================
    // Firmware Loading (1 tool)
    // =========================================================================

    #[tool(description = "Load firmware to target memory (ELF, HEX, or BIN). For M4 targets, this loads to RAM (RETRAM/MCUSRAM) — firmware is lost on power cycle.")]
    async fn load_firmware(&self, Parameters(args): Parameters<LoadFirmwareArgs>) -> Result<CallToolResult, McpError> {
        let file_path = &args.file_path;

        // Validate file exists
        if !Path::new(file_path).exists() {
            return Err(McpError::invalid_params(
                format!("Firmware file not found: {}", file_path),
                None,
            ));
        }

        // Determine file type and build command
        let cmd = if file_path.ends_with(".bin") {
            let addr = args.address.as_deref().ok_or_else(|| {
                McpError::invalid_params(
                    "BIN files require an address parameter (e.g., \"0x10000000\" for MCUSRAM)".to_string(),
                    None,
                )
            })?;
            format!("load_image {} {} bin", file_path, addr)
        } else if file_path.ends_with(".hex") {
            format!("load_image {} 0x0 ihex", file_path)
        } else {
            // ELF — OpenOCD auto-detects
            format!("load_image {}", file_path)
        };

        // Halt before loading
        let _ = self.tcl_command(&args.session_id, "halt").await;

        let output = self.tcl_command(&args.session_id, &cmd).await?;

        info!("Firmware loaded: {}", file_path);
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Firmware loaded: {}\n{}", file_path, output.trim()
        ))]))
    }

    // =========================================================================
    // Memory Operations (2 tools)
    // =========================================================================

    #[tool(description = "Read memory from the target (32-bit words)")]
    async fn read_memory(&self, Parameters(args): Parameters<ReadMemoryArgs>) -> Result<CallToolResult, McpError> {
        let addr = parse_address(&args.address).map_err(|e| make_error(e.to_string()))?;

        let cmd = format!("mdw 0x{:08x} {}", addr, args.count);
        let output = self.tcl_command(&args.session_id, &cmd).await?;

        match args.format.as_str() {
            "words32" => {
                let words = parse_memory_dump(&output);
                let formatted: Vec<String> = words.iter().map(|w| format!("0x{:08x}", w)).collect();
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Memory at 0x{:08x} ({} words):\n{}",
                    addr,
                    words.len(),
                    formatted.join(" ")
                ))]))
            }
            _ => {
                // Raw hex output from OpenOCD
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Memory at 0x{:08x}:\n{}", addr, output.trim()
                ))]))
            }
        }
    }

    #[tool(description = "Write a 32-bit word to target memory")]
    async fn write_memory(&self, Parameters(args): Parameters<WriteMemoryArgs>) -> Result<CallToolResult, McpError> {
        let addr = parse_address(&args.address).map_err(|e| make_error(e.to_string()))?;
        let value = parse_address(&args.value).map_err(|e| make_error(e.to_string()))?;

        let cmd = format!("mww 0x{:08x} 0x{:08x}", addr, value);
        let output = self.tcl_command(&args.session_id, &cmd).await?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Wrote 0x{:08x} to 0x{:08x}\n{}", value as u32, addr, output.trim()
        ))]))
    }

    // =========================================================================
    // Serial Monitor (1 tool)
    // =========================================================================

    #[tool(description = "Capture UART serial output from the target for a specified duration")]
    async fn monitor(&self, Parameters(args): Parameters<MonitorArgs>) -> Result<CallToolResult, McpError> {
        // Verify session exists
        let _ = self.get_session(&args.session_id).await?;

        let port_name = args.port
            .or_else(|| self.config.default_serial_port.clone())
            .ok_or_else(|| McpError::invalid_params(
                "No serial port specified and no default configured. Pass port parameter or use --serial-port CLI flag.".to_string(),
                None,
            ))?;

        info!("Monitoring serial port {} at {} baud for {}s", port_name, args.baud_rate, args.duration_seconds);

        // Open serial port
        let builder = tokio_serial::new(&port_name, args.baud_rate);
        let mut port = tokio_serial::SerialStream::open(&builder).map_err(|e| {
            make_error(format!("Failed to open serial port {}: {}", port_name, e))
        })?;

        // Read for duration
        let mut output = String::new();
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(args.duration_seconds);

        use tokio::io::AsyncReadExt;
        let mut buf = [0u8; 1024];

        loop {
            let remaining = deadline - tokio::time::Instant::now();
            if remaining.is_zero() {
                break;
            }

            match tokio::time::timeout(remaining, port.read(&mut buf)).await {
                Ok(Ok(n)) if n > 0 => {
                    output.push_str(&String::from_utf8_lossy(&buf[..n]));
                }
                Ok(Ok(_)) => break, // EOF
                Ok(Err(e)) => {
                    warn!("Serial read error: {}", e);
                    break;
                }
                Err(_) => break, // Timeout — capture period over
            }
        }

        let message = if output.is_empty() {
            format!("No output captured from {} in {}s", port_name, args.duration_seconds)
        } else {
            format!(
                "Serial output from {} ({}s capture, {} bytes):\n\n{}",
                port_name,
                args.duration_seconds,
                output.len(),
                output
            )
        };

        Ok(CallToolResult::success(vec![Content::text(message)]))
    }
}

#[tool_handler]
impl ServerHandler for OpenocdDebugToolHandler {}
