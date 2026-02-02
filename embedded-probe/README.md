# Embedded Debugger MCP Server

[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://rust-lang.org)
[![RMCP](https://img.shields.io/badge/RMCP-0.3.2-blue.svg)](https://github.com/modelcontextprotocol/rust-sdk)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

A professional Model Context Protocol (MCP) server for embedded debugging with probe-rs. Provides AI assistants with comprehensive debugging capabilities for embedded systems including ARM Cortex-M, RISC-V microcontrollers with real hardware integration.

> 📖 **Language Versions**: [English](README.md) | [中文](README_zh.md)

## ✨ Features

- 🚀 **Production Ready**: Real hardware integration with 27 comprehensive debugging tools
- 🔌 **Multi-Probe Support**: J-Link, ST-Link V2/V3, DAPLink, Black Magic Probe
- 🎯 **Complete Debug Control**: Connect, halt, run, reset, single-step execution
- 💾 **Memory Operations**: Read/write flash and RAM with multiple data formats
- 🛑 **Breakpoint Management**: Hardware and software breakpoints with real-time control
- 📱 **Flash Programming**: Complete flash operations - erase, program, verify
- 📡 **RTT Bidirectional**: Real-Time Transfer with interactive command/response system
- 🏗️ **Multi-Architecture**: ARM Cortex-M, RISC-V with tested STM32 integration
- 🤖 **AI Integration**: Perfect compatibility with Claude and other AI assistants
- 🔧 **Vendor Tools**: ESP32 via esptool, Nordic via nrfjprog for chips needing vendor support
- ✅ **Boot Validation**: Automated flash + reset + RTT pattern matching workflows

## 🏗️ Architecture

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   MCP Client    │◄──►│  Embedded        │◄──►│  Debug Probe    │
│   (Claude/AI)   │    │  Debugger MCP    │    │  Hardware       │
└─────────────────┘    └──────────────────┘    └─────────────────┘
                              │
                              ▼
                       ┌──────────────────┐
                       │  Target Device   │
                       │  (ARM/RISC-V)    │
                       └──────────────────┘
```

## 🚀 Quick Start

### Prerequisites

**Hardware Requirements:**
- **Debug Probe**: ST-Link V2/V3, J-Link, or DAPLink compatible probe
- **Target Board**: STM32 or other supported microcontroller
- **Connection**: USB cables for probe and target board

**Software Requirements:**
- Rust 1.70+ 
- probe-rs compatible debug probe drivers

### Installation

```bash
# Clone and build from source
git clone https://github.com/adancurusul/embedded-debugger-mcp.git
cd embedded-debugger-mcp
cargo build --release
```

### Basic Usage

**Configure MCP Clients**

#### Claude Desktop Configuration Example

Add to Claude Desktop configuration file:

**Windows Example:**
```json
{
  "mcpServers": {
    "embedded-debugger": {
      "command": "C:\\path\\to\\debugger-mcp-rs\\target\\release\\embedded-debugger-mcp.exe",
      "args": [],
      "env": {
        "RUST_LOG": "info"
      }
    }
  }
}
```

**macOS/Linux Example:**
```json
{
  "mcpServers": {
    "embedded-debugger": {
      "command": "/path/to/debugger-mcp-rs/target/release/embedded-debugger-mcp",
      "args": [],
      "env": {
        "RUST_LOG": "info"
      }
    }
  }
}
```

Other examples for other tools like cursor ,claude code  etc. please refer to the corresponding tool documentation

## 🎯 Try the STM32 Demo

We provide a comprehensive **STM32 RTT Bidirectional Demo** that showcases all capabilities:

```bash
# Navigate to the example
cd examples/STM32_demo

# Build the firmware  
cargo build --release

# Use with MCP server for complete debugging experience
```

**What the demo shows:**
- ✅ **Interactive RTT Communication**: Send commands and get real-time responses
- ✅ **Core MCP Tools**: Complete validation with real STM32 hardware
- ✅ **Fibonacci Calculator**: Live data streaming with control commands
- ✅ **Hardware Integration**: Tested with STM32G431CBTx + ST-Link V2

[📖 View STM32 Demo Documentation →](examples/STM32_demo/README.md)

### Usage Examples with AI Assistants

#### List Available Debug Probes
```
Please list available debug probes on the system
```

#### Connect and Flash Firmware
```
Connect to my STM32G431CBTx using ST-Link probe, then flash the firmware at examples/STM32_demo/target/thumbv7em-none-eabi/release/STM32_demo
```

#### Interactive RTT Communication
```
Please attach RTT and show me the data from the terminal channel. Then send a command 'L' to toggle the LED.
```

#### Memory Analysis
```
Read 64 bytes of memory from address 0x08000000 and analyze the data format
```

#### Boot Validation Workflow
```
Flash my firmware and validate it boots correctly by checking for "Booting Zephyr" in the RTT output within 5 seconds.
```

#### ESP32 Flashing (Xtensa chips)
```
Flash my ESP32 firmware using esptool on port /dev/cu.usbserial-1430
```

## 🛠️ Complete Tool Set (27 Tools)

All probe-rs tools tested with real STM32 hardware. Vendor tools require esptool/nrfjprog installed:

### 🔌 Probe Management (3 tools)
| Tool | Description | Status |
|------|-------------|---------|
| `list_probes` | Discover available debug probes | ✅ Production Ready |
| `connect` | Connect to probe and target chip | ✅ Production Ready |
| `probe_info` | Get detailed session information | ✅ Production Ready |

### 💾 Memory Operations (2 tools) 
| Tool | Description | Status |
|------|-------------|---------|
| `read_memory` | Read flash/RAM with multiple formats | ✅ Production Ready |
| `write_memory` | Write to target memory | ✅ Production Ready |

### 🎯 Debug Control (4 tools)
| Tool | Description | Status |
|------|-------------|---------|
| `halt` | Stop target execution | ✅ Production Ready |
| `run` | Resume target execution | ✅ Production Ready |
| `reset` | Hardware/software reset | ✅ Production Ready |
| `step` | Single instruction stepping | ✅ Production Ready |

### 🛑 Breakpoint Management (2 tools)
| Tool | Description | Status |
|------|-------------|---------|
| `set_breakpoint` | Set hardware/software breakpoints | ✅ Production Ready |
| `clear_breakpoint` | Remove breakpoints | ✅ Production Ready |

### 📱 Flash Operations (3 tools)
| Tool | Description | Status |
|------|-------------|---------|
| `flash_erase` | Erase flash memory sectors/chip | ✅ Production Ready |
| `flash_program` | Program ELF/HEX/BIN files | ✅ Production Ready |
| `flash_verify` | Verify flash contents | ✅ Production Ready |

### 📡 RTT Communication (6 tools)
| Tool | Description | Status |
|------|-------------|---------|
| `rtt_attach` | Connect to RTT communication | ✅ Production Ready |
| `rtt_detach` | Disconnect RTT | ✅ Production Ready |
| `rtt_channels` | List available RTT channels | ✅ Production Ready |
| `rtt_read` | Read from RTT up channels | ✅ Production Ready |
| `rtt_write` | Write to RTT down channels | ✅ Production Ready |
| `run_firmware` | Complete deployment + RTT | ✅ Production Ready |

### 📊 Session Management (2 tools)
| Tool | Description | Status |
|------|-------------|---------|
| `get_status` | Get current debug status | ✅ Production Ready |
| `disconnect` | Clean session termination | ✅ Production Ready |

### ✅ Boot Validation (1 tool)
| Tool | Description | Status |
|------|-------------|---------|
| `validate_boot` | Flash + reset + RTT pattern matching | ✅ Production Ready |

### 🔧 Vendor Tools (4 tools)
| Tool | Description | Requires |
|------|-------------|----------|
| `esptool_flash` | Flash ESP32 Xtensa chips via serial | esptool.py |
| `esptool_monitor` | Read ESP32 serial output | pyserial |
| `nrfjprog_flash` | Flash Nordic chips via J-Link | nRF Command Line Tools |
| `load_custom_target` | Load custom target YAML for probe-rs | - |

**✅ 27 Tools Total**

## 🌍 Supported Hardware

### Chip Support Matrix

| Chip Family | probe-rs | Vendor Tool | Notes |
|-------------|----------|-------------|-------|
| STM32 | ✅ Primary | - | Full support via SWD/JTAG |
| nRF52/53/54 | ✅ Primary | nrfjprog | Use nrfjprog for J-Link specific features |
| ESP32-C3/C6 | ✅ Primary | esptool | RISC-V chips work well in probe-rs |
| ESP32/S2/S3 | ⚠️ Limited | esptool | Xtensa - use esptool for reliable flash |
| Alif | ✅ Primary | - | CMSIS-DAP + J-Link supported |

### Debug Probes
- **J-Link**: Segger J-Link (all variants)
- **ST-Link**: ST-Link/V2, ST-Link/V3
- **DAPLink**: ARM DAPLink compatible probes
- **CMSIS-DAP**: Standard CMSIS-DAP probes
- **Black Magic Probe**: Black Magic Probe

### Target Architectures
- **ARM Cortex-M**: M0, M0+, M3, M4, M7, M23, M33
- **RISC-V**: ESP32-C3/C6, various RISC-V cores
- **ARM Cortex-A**: Basic support

## 🔧 Vendor Tool Installation

For ESP32 Xtensa chips (ESP32, ESP32-S2, ESP32-S3):
```bash
pip install esptool pyserial
```

For Nordic chips with J-Link features:
```bash
# Download from: https://www.nordicsemi.com/Products/Development-tools/nRF-Command-Line-Tools
# macOS: brew install --cask nordic-nrf-command-line-tools
```

## 🏆 Production Status

### ✅ Fully Implemented and Tested

**Current Status: PRODUCTION READY**

- ✅ **Complete probe-rs Integration**: Real hardware debugging with 22 core tools
- ✅ **Vendor Tool Support**: 5 additional tools for ESP32 Xtensa and Nordic
- ✅ **Boot Validation**: Automated flash + RTT pattern matching workflows
- ✅ **Hardware Validation**: Tested with STM32G431CBTx + ST-Link V2
- ✅ **RTT Bidirectional**: Full interactive communication with real-time commands
- ✅ **Flash Operations**: Complete erase, program, verify workflow
- ✅ **Session Management**: Multi-session support with robust error handling
- ✅ **AI Integration**: Perfect MCP protocol compatibility

## 🙏 Acknowledgments

Thanks to the following open source projects:

- [probe-rs](https://probe.rs/) - Embedded debugging toolkit
- [rmcp](https://github.com/modelcontextprotocol/rust-sdk) - Rust MCP SDK
- [tokio](https://tokio.rs/) - Async runtime

## 📄 License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.

---

⭐ If this project helps you, please give us a Star!