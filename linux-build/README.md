# Linux Build MCP Server

MCP server for Docker-based Linux cross-compilation and SSH deployment. Manages container lifecycle, runs builds inside containers, and deploys artifacts to target boards via SSH/SCP.

## Tools (9)

| Tool | Description |
|------|-------------|
| `start_container` | Start a Docker build container with workspace mount |
| `stop_container` | Stop and remove a container |
| `container_status` | Check if a container is running |
| `run_command` | Execute a command inside the container |
| `build` | Run a build command in the container (default: `make`) |
| `list_artifacts` | List files in the container's artifacts directory |
| `collect_artifacts` | Copy build artifacts from container to host |
| `deploy` | Deploy a file to the board via SCP |
| `ssh_command` | Run a command on the board via SSH |

## Quick Start

```bash
# Build
cargo build --release

# Run (with Docker image and workspace)
./target/release/linux-build \
  --docker-image stm32mp1-sdk \
  --workspace-dir /path/to/source \
  --board-ip 192.168.1.100
```

## Configuration

| CLI Flag | Default | Description |
|----------|---------|-------------|
| `--docker-image` | `stm32mp1-sdk` | Docker image for build environment |
| `--workspace-dir` | — | Host directory mounted at `/workspace` |
| `--board-ip` | — | Default board IP for deploy/ssh |
| `--ssh-key` | — | SSH private key path |
| `--ssh-user` | `root` | SSH user for board access |
| `--log-level` | `info` | Log level |
| `--log-file` | stderr | Log file path |

## Example Usage

```json
// Start a build container
{"method": "tools/call", "params": {"name": "start_container", "arguments": {
  "workspace_dir": "/path/to/source"
}}}

// Run a build
{"method": "tools/call", "params": {"name": "build", "arguments": {
  "container": "linux-build-abc12345",
  "command": "make -j$(nproc)",
  "workdir": "/workspace"
}}}

// List build artifacts
{"method": "tools/call", "params": {"name": "list_artifacts", "arguments": {
  "container": "linux-build-abc12345"
}}}

// Copy artifacts to host
{"method": "tools/call", "params": {"name": "collect_artifacts", "arguments": {
  "container": "linux-build-abc12345",
  "host_path": "/tmp/artifacts"
}}}

// Deploy to board
{"method": "tools/call", "params": {"name": "deploy", "arguments": {
  "file_path": "/tmp/artifacts/app.elf",
  "board_ip": "192.168.1.100"
}}}

// Run command on board
{"method": "tools/call", "params": {"name": "ssh_command", "arguments": {
  "command": "systemctl restart my-app",
  "board_ip": "192.168.1.100"
}}}

// Stop container when done
{"method": "tools/call", "params": {"name": "stop_container", "arguments": {
  "container": "linux-build-abc12345"
}}}
```

## Requirements

- Docker
- SSH client (for deploy/ssh_command tools)

## Docker Image

The server expects a Docker image with the cross-compilation toolchain pre-installed. The container:
- Mounts the host workspace at `/workspace`
- Creates `/artifacts` for build outputs
- Runs `sleep infinity` to stay alive for `docker exec` commands

Example Dockerfile for STM32MP1:
```dockerfile
FROM ubuntu:22.04
RUN apt-get update && apt-get install -y build-essential crossbuild-essential-armhf
WORKDIR /workspace
```
