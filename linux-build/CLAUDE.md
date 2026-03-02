# linux-build

Docker-based Linux cross-compilation, ADB/SSH deployment, Yocto build tracking, and board connection management MCP server.

## Build

```bash
cargo build --release
```

## Tools (18)

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
- `kernel_rebuild` — Force-rebuild kernel after config changes (configure -f → compile -f → deploy -f), with optional image rebuild and config verification

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

Container creation with meta-eai bind mount (host edits propagate automatically):
```bash
docker run -dit --name yocto-build \
  -v yocto-data:/home/builder/yocto \
  -v /Users/danahern/code/claude/work/yocto-build:/home/builder/artifacts \
  -v /Users/danahern/code/claude/work/firmware/linux/yocto/meta-eai:/home/builder/yocto/meta-eai \
  yocto-builder \
  bash -c "sleep infinity"
```

For recipe-only changes:
```
yocto_build(container="yocto-build", build_dir="build-alif-e7", image="alif-tiny-image")
```

For kernel config changes (`.cfg` fragments):
```
kernel_rebuild(
  container="yocto-build",
  image="alif-tiny-image",
  verify_configs=["CONFIG_JFFS2_FS=y", "CONFIG_MTD_PHRAM=y"]
)
```

`kernel_rebuild` parameters:
- `container` (required) — Container name
- `build_dir` (default: "build-alif-e7") — Yocto build directory
- `recipe` (default: "linux-alif") — Kernel recipe name
- `image` (optional) — Image to rebuild after kernel (e.g. "alif-tiny-image")
- `verify_configs` (optional) — CONFIG_ options to verify in .config after build
- `background` (default: false) — Run in background (use `yocto_build_status` to check)

## Architecture

- `config.rs` — CLI args (clap) and runtime config (board IP, ADB serial, SSH)
- `docker_client.rs` — Docker CLI wrapper (start/stop/exec/cp) + SSH/SCP + flash_image_ssh
- `adb_client.rs` — ADB CLI wrapper (shell/push/pull/devices/flash_image_adb)
- `tools/types.rs` — Serde/JsonSchema arg types for all 18 tools
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
