# Linux Build MCP Server

MCP server for Docker-based Linux cross-compilation with ADB/SSH deployment, Yocto build tracking, and board connection management.

## Tools (17)

| Tool | Description |
|------|-------------|
| `start_container` | Start a Docker build container with workspace mount and extra volumes |
| `stop_container` | Stop and remove a container |
| `container_status` | Check if a container is running |
| `run_command` | Execute a command inside the container |
| `build` | Run a build command in the container (default: `make`) |
| `list_artifacts` | List files in the container's artifacts directory |
| `collect_artifacts` | Copy build artifacts from container to host |
| `deploy` | Deploy a file to the board via SCP |
| `ssh_command` | Run a command on the board via SSH |
| `adb_shell` | Run a shell command on the board via ADB |
| `adb_deploy` | Push a file to the board via ADB |
| `adb_pull` | Pull a file from the board via ADB |
| `flash_image` | Flash a compressed WIC image via SSH or ADB |
| `yocto_build` | Run a bitbake build (foreground or background) |
| `yocto_build_status` | Check background Yocto build status |
| `board_connect` | Register a board connection (SSH/ADB/auto) |
| `board_disconnect` | Remove a board connection |
| `board_status` | Check board connection status |

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
| `--adb-serial` | — | Default ADB device serial number |
| `--log-level` | `info` | Log level |
| `--log-file` | stderr | Log file path |

## Example Workflows

### Cross-compilation + SSH Deploy

```
start_container(workspace_dir="/path/to/source")
build(container, command="make -j$(nproc)")
collect_artifacts(container, host_path="/tmp/artifacts")
deploy(file_path="/tmp/artifacts/app", board_ip="192.168.1.100")
ssh_command(command="systemctl restart my-app", board_ip="192.168.1.100")
stop_container(container)
```

### ADB Workflow

```
adb_shell(command="uname -a")
adb_deploy(file_path="/tmp/app", remote_path="/data/local/tmp/")
adb_shell(command="/data/local/tmp/app")
adb_pull(remote_path="/tmp/log.txt", local_path="/tmp/log.txt")
```

### Yocto Build with meta-eai

```
start_container(
  image="yocto-builder",
  extra_volumes=[
    "yocto-data:/home/builder/yocto",
    "/path/to/meta-eai:/home/builder/yocto/meta-eai"
  ]
)
yocto_build(container, build_dir="build-stm32mp1", background=true)
yocto_build_status(build_id="abc12345")
```

### Flash Image

```
flash_image(image_path="/tmp/image.wic.bz2", transport="ssh", board_ip="192.168.7.2")
flash_image(image_path="/tmp/image.wic.bz2", transport="adb")
```

### Board Connection

```
board_connect(transport="auto")                    # tries ADB, falls back to SSH
board_connect(transport="ssh", board_ip="10.0.0.1")
board_status()                                      # list all connections
board_disconnect(board_id="abc12345")
```

## Requirements

- Docker
- SSH client (for deploy/ssh_command/flash_image SSH transport)
- ADB (for adb_shell/adb_deploy/adb_pull/flash_image ADB transport)

## Docker Image

The server expects a Docker image with the cross-compilation toolchain pre-installed. The container:
- Mounts the host workspace at `/workspace`
- Creates `/artifacts` for build outputs
- Runs `sleep infinity` to stay alive for `docker exec` commands
- Supports extra volumes via `extra_volumes` parameter (named or bind mounts)

Example Dockerfile for STM32MP1:
```dockerfile
FROM ubuntu:22.04
RUN apt-get update && apt-get install -y build-essential crossbuild-essential-armhf
WORKDIR /workspace
```
