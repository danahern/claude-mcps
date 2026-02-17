//! Type definitions for OpenOCD debug MCP tools

use serde::Deserialize;
use schemars::JsonSchema;

// ============================================================================
// connect
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConnectArgs {
    /// Path to OpenOCD config file (e.g., "board/stm32mp15_dk2.cfg")
    pub cfg_file: String,
    /// Extra OpenOCD command-line arguments
    #[serde(default)]
    pub extra_args: Option<Vec<String>>,
}

// ============================================================================
// disconnect
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DisconnectArgs {
    /// Session ID to disconnect
    pub session_id: String,
}

// ============================================================================
// get_status
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetStatusArgs {
    /// Session ID
    pub session_id: String,
}

// ============================================================================
// halt
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct HaltArgs {
    /// Session ID
    pub session_id: String,
}

// ============================================================================
// run
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RunArgs {
    /// Session ID
    pub session_id: String,
}

// ============================================================================
// reset
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ResetArgs {
    /// Session ID
    pub session_id: String,
    /// Whether to halt after reset (default: true)
    #[serde(default = "default_true")]
    pub halt_after_reset: bool,
}

fn default_true() -> bool { true }

// ============================================================================
// load_firmware
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LoadFirmwareArgs {
    /// Session ID
    pub session_id: String,
    /// Path to firmware file (ELF, HEX, or BIN)
    pub file_path: String,
    /// Base address for BIN files (hex string like "0x10000000"). Required for .bin, ignored for ELF/HEX.
    #[serde(default)]
    pub address: Option<String>,
}

// ============================================================================
// read_memory
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadMemoryArgs {
    /// Session ID
    pub session_id: String,
    /// Memory address (hex string like "0x10000000" or decimal)
    pub address: String,
    /// Number of 32-bit words to read
    #[serde(default = "default_word_count")]
    pub count: u32,
    /// Output format: "hex" (default), "words32"
    #[serde(default = "default_format")]
    pub format: String,
}

fn default_word_count() -> u32 { 1 }
fn default_format() -> String { "hex".to_string() }

// ============================================================================
// write_memory
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WriteMemoryArgs {
    /// Session ID
    pub session_id: String,
    /// Memory address (hex string like "0x10000000" or decimal)
    pub address: String,
    /// 32-bit value to write (hex string like "0xDEADBEEF" or decimal)
    pub value: String,
}

// ============================================================================
// monitor (serial console)
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MonitorArgs {
    /// Session ID
    pub session_id: String,
    /// Serial port (e.g., "/dev/ttyACM0"). Uses default from config if omitted.
    #[serde(default)]
    pub port: Option<String>,
    /// Baud rate (default: 115200)
    #[serde(default = "default_baud")]
    pub baud_rate: u32,
    /// Capture duration in seconds (default: 5)
    #[serde(default = "default_duration")]
    pub duration_seconds: u64,
}

fn default_baud() -> u32 { 115200 }
fn default_duration() -> u64 { 5 }
