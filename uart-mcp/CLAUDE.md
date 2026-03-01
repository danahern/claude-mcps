# uart-mcp

Bidirectional UART MCP server — session-based serial console interaction for Linux shells, U-Boot, AT modems, and other serial devices.

## Setup

```bash
pip install -e ".[dev]"
```

## Tools

### Port Discovery
- `list_ports()` — List available serial ports with device path, description, manufacturer

### Session Management
- `open_port(port, baud?, echo_filter?)` — Open a serial session, returns session_id
- `close_port(session_id)` — Close session and release port

### Communication
- `send_command(session_id, command, timeout?, wait_for?)` — Send command, collect response
- `read_output(session_id, timeout?)` — Read pending output (non-blocking drain)
- `write_raw(session_id, data, hex?)` — Write raw bytes without waiting

## Response Detection

`send_command` uses two strategies:
1. **Idle timeout** (default 0.5s) — stops reading after no new bytes for `timeout` seconds
2. **Regex prompt matching** (`wait_for`) — stops immediately on pattern match

Common `wait_for` patterns:
- `[#$] $` — Linux shell prompt
- `=> $` — U-Boot prompt
- `OK\r\n` — AT modem OK response
- `login: $` — Login prompt

## Typical Workflows

### Linux Shell
```
1. uart.open_port(port="/dev/cu.usbserial-AO009AHE", baud=115200)
2. uart.send_command(session_id="...", command="uname -a", wait_for="# $")
3. uart.send_command(session_id="...", command="ifconfig eth0", wait_for="# $")
4. uart.close_port(session_id="...")
```

### U-Boot Console
```
1. uart.open_port(port="/dev/cu.usbserial-AO009AHE", baud=115200)
2. uart.send_command(session_id="...", command="printenv bootargs", wait_for="=> $")
3. uart.close_port(session_id="...")
```

### Raw Binary Protocol
```
1. uart.open_port(port="/dev/cu.usbserial-123", baud=9600, echo_filter=false)
2. uart.write_raw(session_id="...", data="414243", hex=true)
3. uart.read_output(session_id="...", timeout=1.0)
4. uart.close_port(session_id="...")
```

## Testing

```bash
python3 -m pytest tests/ -v
```
