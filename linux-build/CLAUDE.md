# linux-build

Docker-based Linux cross-compilation, ADB/SSH deployment, Yocto build tracking, and board connection management MCP server.

## Build

```bash
cargo build --release
```

## Tools (17)

### Container Lifecycle
- `start_container` — Start Docker container with optional workspace mount and extra volumes, returns container name
- `stop_container` — Stop and remove a container
- `container_status` — Check container state (running/exited/not found)

### Build Operations
- `run_command` — Execute arbitrary command in container via `docker exec`
- `build` — Run build command in container (default: `make` in `/workspace`)
- `list_artifacts` — List files in container's `/artifacts` directory

### SSH Deployment
- `collect_artifacts` — Copy files from container to host via `docker cp`
- `deploy` — SCP file to board (requires board_ip from arg or --board-ip)
- `ssh_command` — Run command on board via SSH

### ADB Transport
- `adb_shell` — Run shell command on device via ADB
- `adb_deploy` — Push file to device via ADB
- `adb_pull` — Pull file from device via ADB

### Flash Image
- `flash_image` — Stream compressed WIC image to board via SSH or ADB (`bzcat | dd`)

### Yocto Build
- `yocto_build` — Run bitbake in container (foreground or background mode)
- `yocto_build_status` — Check background build status, elapsed time, and truncated output

### Board Connection
- `board_connect` — Register SSH/ADB/auto board connection, returns board_id
- `board_disconnect` — Remove a board connection
- `board_status` — Check connection status (single or list all)

## Multi-Board Usage

The `image` parameter in `start_container` selects the Docker image per board:

| Board | Docker Image | Build Command |
|-------|-------------|---------------|
| STM32MP1 | `stm32mp1-sdk` | `make -C /workspace/firmware/linux/apps all install` |
| Alif E7 | `alif-e7-sdk:latest` (workspace default) | `make -C /workspace/firmware/linux/apps BOARD=alif-e7 all install` |

### Yocto builds with meta-eai

```
start_container(
  name="yocto-build",
  image="yocto-builder",
  extra_volumes=[
    "yocto-data:/home/builder/yocto",
    "/path/to/firmware/linux/yocto/meta-eai:/home/builder/yocto/meta-eai"
  ]
)
yocto_build(container="yocto-build", build_dir="build-alif-e7", background=true)
yocto_build_status(build_id="...")
```

The bind mount overlays the named volume path, so `bblayers.conf` references resolve correctly.

## Architecture

- `config.rs` — CLI args (clap) and runtime config (board IP, ADB serial, SSH)
- `docker_client.rs` — Docker CLI wrapper (start/stop/exec/cp) + SSH/SCP + flash_image_ssh
- `adb_client.rs` — ADB CLI wrapper (shell/push/pull/devices/flash_image_adb)
- `tools/types.rs` — Serde/JsonSchema arg types for all 17 tools
- `tools/linux_build_tools.rs` — RMCP tool handler, Yocto build state, board connection state
- `main.rs` — Entry point, logging setup

## Key Details

- Containers mount host workspace at `/workspace` and create `/artifacts` for outputs
- Container name auto-generated as `linux-build-<uuid8>` if not provided
- `extra_volumes` on `start_container` adds arbitrary `-v` mounts (named or bind)
- Deploy/SSH tools require board IP — pass as arg or configure with `--board-ip`
- ADB tools use `--adb-serial` default or per-call `serial` parameter
- Yocto builds track state in `Arc<RwLock<HashMap>>` — background builds via `tokio::spawn`
- Board connections store transport details; subsequent tools can reference `board_id`
- Flash image uses `spawn_blocking` to pipe `bzcat` stdout into SSH/ADB `dd`
- Default Docker image: `alif-e7-sdk:latest` (workspace-configured via `--docker-image`; code default is `stm32mp1-sdk`)
