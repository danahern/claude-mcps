//! Type definitions for embedded debugger MCP tools

use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

// =============================================================================
// Debugger Management Types
// =============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListProbesArgs {
    // No parameters needed
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConnectArgs {
    /// Probe selector (serial number, identifier, or "auto" for first available)
    pub probe_selector: String,
    /// Target chip name (e.g., "STM32F407VGTx", "nRF52840_xxAA")
    pub target_chip: String,
    /// Connection speed in kHz (default: 4000)
    #[serde(default = "default_speed_khz")]
    pub speed_khz: u32,
    /// Whether to connect under reset
    #[serde(default)]
    pub connect_under_reset: bool,
    /// Whether to halt after connecting
    #[serde(default = "default_true")]
    pub halt_after_connect: bool,
}

fn default_speed_khz() -> u32 { 4000 }
fn default_true() -> bool { true }

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DisconnectArgs {
    /// Session ID to disconnect
    pub session_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProbeInfoArgs {
    /// Session ID to get info for
    pub session_id: String,
}

// =============================================================================
// Target Control Types
// =============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct HaltArgs {
    /// Session ID
    pub session_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RunArgs {
    /// Session ID
    pub session_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ResetArgs {
    /// Session ID
    pub session_id: String,
    /// Reset type: "hardware" or "software"
    #[serde(default = "default_reset_type")]
    pub reset_type: String,
    /// Whether to halt after reset
    #[serde(default = "default_true")]
    pub halt_after_reset: bool,
}

fn default_reset_type() -> String { "hardware".to_string() }

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StepArgs {
    /// Session ID
    pub session_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetStatusArgs {
    /// Session ID
    pub session_id: String,
}

// =============================================================================
// Memory Operation Types
// =============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadMemoryArgs {
    /// Session ID
    pub session_id: String,
    /// Memory address (hex string like "0x8000000" or decimal)
    pub address: String,
    /// Number of bytes to read
    pub size: usize,
    /// Output format: "hex", "binary", "ascii", "words32", "words16"
    #[serde(default = "default_format")]
    pub format: String,
}

fn default_format() -> String { "hex".to_string() }

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WriteMemoryArgs {
    /// Session ID
    pub session_id: String,
    /// Memory address (hex string like "0x8000000" or decimal)
    pub address: String,
    /// Data to write
    pub data: String,
    /// Input format: "hex", "binary", "ascii", "words32", "words16"
    #[serde(default = "default_format")]
    pub format: String,
}


// =============================================================================
// Breakpoint Management Types
// =============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetBreakpointArgs {
    /// Session ID
    pub session_id: String,
    /// Breakpoint address (hex string like "0x8000000" or decimal)
    pub address: String,
    /// Breakpoint type: "hardware" or "software"
    #[serde(default = "default_breakpoint_type")]
    pub breakpoint_type: String,
}

fn default_breakpoint_type() -> String { "hardware".to_string() }

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ClearBreakpointArgs {
    /// Session ID
    pub session_id: String,
    /// Breakpoint address (hex string like "0x8000000" or decimal)
    pub address: String,
}


// =============================================================================
// Flash Programming Types
// =============================================================================



// =============================================================================
// New Flash Programming Types
// =============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FlashEraseArgs {
    /// Session ID
    pub session_id: String,
    /// Erase type: "all" for full chip, "sectors" for specific sectors
    #[serde(default = "default_erase_all")]
    pub erase_type: String,
    /// Start address for sector erase (hex string like "0x8000000" or decimal)
    pub address: Option<String>,
    /// Size in bytes for sector erase
    pub size: Option<u32>,
}

fn default_erase_all() -> String { "all".to_string() }

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FlashProgramArgs {
    /// Session ID
    pub session_id: String,
    /// Path to file to program (ELF, HEX, BIN)
    pub file_path: String,
    /// File format: "auto", "elf", "hex", "bin"
    #[serde(default = "default_auto_format")]
    pub format: String,
    /// Base address for BIN files (hex string or decimal)
    pub base_address: Option<String>,
    /// Whether to verify after programming
    #[serde(default = "default_true")]
    pub verify: bool,
}

fn default_auto_format() -> String { "auto".to_string() }

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FlashVerifyArgs {
    /// Session ID
    pub session_id: String,
    /// File path to verify against (optional)
    pub file_path: Option<String>,
    /// Hex data to verify against (alternative to file_path)
    pub data: Option<String>,
    /// Address to start verification (hex string or decimal)
    pub address: String,
    /// Number of bytes to verify
    pub size: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RunFirmwareArgs {
    /// Session ID
    pub session_id: String,
    /// Path to firmware file
    pub file_path: String,
    /// File format: "auto", "elf", "hex", "bin"
    #[serde(default = "default_auto_format")]
    pub format: String,
    /// Whether to reset after flashing
    #[serde(default = "default_true")]
    pub reset_after_flash: bool,
    /// Whether to attach RTT after reset
    #[serde(default = "default_true")]
    pub attach_rtt: bool,
    /// RTT attach timeout in milliseconds
    #[serde(default = "default_rtt_timeout")]
    pub rtt_timeout_ms: u32,
}

fn default_rtt_timeout() -> u32 { 3000 }

// =============================================================================
// RTT Communication Types
// =============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RttAttachArgs {
    /// Session ID
    pub session_id: String,
    /// RTT control block address (optional, auto-detected if not provided)
    pub control_block_address: Option<String>,
    /// Memory ranges to search for RTT control block
    /// Each range is a tuple of (start_address, end_address)
    pub memory_ranges: Option<Vec<MemoryRange>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MemoryRange {
    pub start: String,
    pub end: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RttDetachArgs {
    /// Session ID
    pub session_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RttReadArgs {
    /// Session ID
    pub session_id: String,
    /// RTT channel number (usually 0 for default output)
    #[serde(default)]
    pub channel: u32,
    /// Maximum bytes to read
    #[serde(default = "default_max_bytes")]
    pub max_bytes: usize,
    /// Timeout in milliseconds
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

fn default_max_bytes() -> usize { 1024 }
fn default_timeout_ms() -> u64 { 1000 }

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RttWriteArgs {
    /// Session ID
    pub session_id: String,
    /// RTT channel number (usually 0 for default input)
    #[serde(default)]
    pub channel: u32,
    /// Data to write
    pub data: String,
    /// Data encoding: "utf8", "hex", "binary"
    #[serde(default = "default_encoding")]
    pub encoding: String,
}

fn default_encoding() -> String { "utf8".to_string() }

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RttChannelsArgs {
    /// Session ID
    pub session_id: String,
}

// =============================================================================
// Response Types (for internal use)
// =============================================================================

#[derive(Debug, Serialize)]
pub struct ProbeInfo {
    pub identifier: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub serial_number: Option<String>,
    pub probe_type: String,
    pub speed_khz: u32,
    pub version: String,
}

#[derive(Debug, Serialize)]
pub struct TargetInfo {
    pub chip_name: String,
    pub architecture: String,
    pub core_type: String,
    pub memory_map: Vec<MemoryRegion>,
}

#[derive(Debug, Serialize)]
pub struct MemoryRegion {
    pub name: String,
    pub start: u64,
    pub size: u64,
    pub access: String,
}

#[derive(Debug, Serialize)]
pub struct CoreInfo {
    pub pc: u64,
    pub sp: u64,
    pub state: String,
    pub halt_reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SessionStatus {
    pub session_id: String,
    pub connected: bool,
    pub target_state: String,
    pub created_at: String,
    pub last_activity: String,
}

#[derive(Debug, Serialize)]
pub struct RegisterValue {
    pub name: String,
    pub value: u64,
    pub description: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Breakpoint {
    pub id: u32,
    pub address: u64,
    pub breakpoint_type: String,
    pub enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct FlashResult {
    pub bytes_programmed: usize,
    pub programming_time_ms: u64,
    pub verification_result: bool,
}

#[derive(Debug, Serialize)]
pub struct RttChannelInfo {
    pub channel: u32,
    pub name: String,
    pub direction: String, // "up", "down"
    pub buffer_size: usize,
    pub flags: u32,
}

// =============================================================================
// Boot Validation Types
// =============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ValidateBootArgs {
    /// Session ID
    pub session_id: String,
    /// Path to firmware file
    pub file_path: String,
    /// Pattern to match in RTT output (string or regex)
    pub success_pattern: String,
    /// Timeout in milliseconds for boot validation
    #[serde(default = "default_boot_timeout")]
    pub timeout_ms: u32,
    /// RTT channel to monitor
    #[serde(default)]
    pub rtt_channel: u32,
    /// Whether to capture full RTT output
    #[serde(default = "default_true")]
    pub capture_output: bool,
}

fn default_boot_timeout() -> u32 { 10000 }

#[derive(Debug, Serialize)]
pub struct BootValidationResult {
    pub success: bool,
    pub boot_time_ms: u64,
    pub matched_pattern: Option<String>,
    pub rtt_output: Option<String>,
    pub error_messages: Vec<String>,
}

// =============================================================================
// Vendor Tool Types (esptool, nrfjprog)
// =============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EsptoolFlashArgs {
    /// Serial port (e.g., "/dev/ttyUSB0", "COM3")
    pub port: String,
    /// Path to firmware file
    pub file_path: String,
    /// ESP32 chip type: esp32, esp32s2, esp32s3, esp32c3, esp32c6
    pub chip: String,
    /// Baud rate for flashing
    #[serde(default = "default_esptool_baud")]
    pub baud_rate: u32,
    /// Flash address (default 0x0 for bin files)
    #[serde(default = "default_flash_address")]
    pub address: String,
    /// Whether to verify after flashing
    #[serde(default = "default_true")]
    pub verify: bool,
    /// Whether to reset after flashing
    #[serde(default = "default_true")]
    pub reset_after: bool,
}

fn default_esptool_baud() -> u32 { 921600 }
fn default_flash_address() -> String { "0x0".to_string() }

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EsptoolMonitorArgs {
    /// Serial port (e.g., "/dev/ttyUSB0", "COM3")
    pub port: String,
    /// Baud rate for serial communication
    #[serde(default = "default_monitor_baud")]
    pub baud_rate: u32,
    /// Timeout in milliseconds (0 = read once and return)
    #[serde(default = "default_monitor_timeout")]
    pub timeout_ms: u64,
    /// Maximum bytes to read
    #[serde(default = "default_monitor_max_bytes")]
    pub max_bytes: usize,
}

fn default_monitor_baud() -> u32 { 115200 }
fn default_monitor_timeout() -> u64 { 1000 }
fn default_monitor_max_bytes() -> usize { 4096 }

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NrfjprogFlashArgs {
    /// Path to firmware file (hex or bin)
    pub file_path: String,
    /// Nordic device family: NRF52, NRF53, NRF54, NRF91
    pub family: String,
    /// Serial number for multi-device setups (optional)
    pub snr: Option<String>,
    /// Whether to verify after flashing
    #[serde(default = "default_true")]
    pub verify: bool,
    /// Whether to reset after flashing
    #[serde(default = "default_true")]
    pub reset_after: bool,
    /// Use sector erase instead of chip erase
    #[serde(default = "default_true")]
    pub sectorerase: bool,
}

// =============================================================================
// nrfutil Vendor Tool Types
// =============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NrfutilProgramArgs {
    /// Path to firmware file (hex or bin)
    pub file_path: String,
    /// Core to program: "Application" or "Network" (for nRF5340 dual-core). Auto-detected if omitted.
    pub core: Option<String>,
    /// Device serial number for multi-device setups (optional)
    pub snr: Option<String>,
    /// Whether to verify after programming
    #[serde(default = "default_true")]
    pub verify: bool,
    /// Whether to reset after programming
    #[serde(default = "default_true")]
    pub reset_after: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NrfutilRecoverArgs {
    /// Device serial number for multi-device setups (optional)
    pub snr: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NrfutilResetArgs {
    /// Device serial number for multi-device setups (optional)
    pub snr: Option<String>,
}

// =============================================================================
// Custom Target Types
// =============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LoadCustomTargetArgs {
    /// Path to target YAML file (probe-rs format)
    pub target_file_path: String,
}

// =============================================================================
// Coredump Analysis Types
// =============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AnalyzeCoredumpArgs {
    /// Raw log/RTT text containing #CD: prefixed coredump lines
    pub log_text: String,
    /// Path to ELF firmware file for symbol resolution
    pub elf_path: String,
}

// =============================================================================
// Advanced Debugging Types
// =============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadRegistersArgs {
    /// Session ID
    pub session_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WriteRegisterArgs {
    /// Session ID
    pub session_id: String,
    /// Register name (e.g., "R0", "SP", "LR", "PC", "xPSR", "R7")
    pub register: String,
    /// Value to write (hex string like "0xDEADBEEF" or decimal)
    pub value: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ResolveSymbolArgs {
    /// Memory address to resolve (hex string like "0x8001234" or decimal)
    pub address: String,
    /// Path to ELF file containing symbols
    pub elf_path: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StackTraceArgs {
    /// Session ID
    pub session_id: String,
    /// Path to ELF file for symbol resolution (optional)
    pub elf_path: Option<String>,
    /// Maximum number of stack frames to collect
    #[serde(default = "default_max_frames")]
    pub max_frames: u32,
}

fn default_max_frames() -> u32 { 32 }

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetWatchpointArgs {
    /// Session ID
    pub session_id: String,
    /// Memory address to watch (hex string like "0x20000000" or decimal)
    pub address: String,
    /// Size of watched region in bytes: 1, 2, or 4
    #[serde(default = "default_watchpoint_size")]
    pub size: u32,
    /// Access type: "read", "write", or "readwrite"
    #[serde(default = "default_watchpoint_access")]
    pub access: String,
}

fn default_watchpoint_size() -> u32 { 4 }
fn default_watchpoint_access() -> String { "readwrite".to_string() }

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ClearWatchpointArgs {
    /// Session ID
    pub session_id: String,
    /// DWT comparator index (0-3) returned by set_watchpoint
    pub index: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CoreDumpArgs {
    /// Session ID
    pub session_id: String,
    /// Output file path for the core dump
    pub output_path: String,
    /// Path to ELF file — if provided, produces GDB-compatible ELF core dump; otherwise raw dump
    pub elf_path: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GdbServerArgs {
    /// Target chip name (e.g., "nRF52840_xxAA", "STM32F407VGTx")
    pub target_chip: String,
    /// Probe selector (serial number, identifier, or "auto" for first available)
    #[serde(default = "default_probe_selector")]
    pub probe_selector: String,
    /// GDB server port number
    #[serde(default = "default_gdb_port")]
    pub port: u16,
    /// Path to ELF file for symbol information (optional)
    pub elf_path: Option<String>,
}

fn default_probe_selector() -> String { "auto".to_string() }
fn default_gdb_port() -> u16 { 1337 }