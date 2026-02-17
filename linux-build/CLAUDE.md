# linux-build

Docker-based Linux cross-compilation and SSH deployment MCP server. Wraps Docker CLI for container lifecycle and SSH/SCP for board deployment.

## Build

```bash
cargo build --release
```

## Tools

- `start_container` — Start Docker container with optional workspace mount, returns container name
- `stop_container` — Stop and remove a container
- `container_status` — Check container state (running/exited/not found)
- `run_command` — Execute arbitrary command in container via `docker exec`
- `build` — Run build command in container (default: `make` in `/workspace`)
- `list_artifacts` — List files in container's `/artifacts` directory
- `collect_artifacts` — Copy files from container to host via `docker cp`
- `deploy` — SCP file to board (requires board_ip from arg or --board-ip)
- `ssh_command` — Run command on board via SSH

## Architecture

- `config.rs` — CLI args (clap) and runtime config
- `docker_client.rs` — Docker CLI wrapper (start/stop/exec/cp) + SSH/SCP functions
- `tools/types.rs` — Serde/JsonSchema arg types for each tool
- `tools/linux_build_tools.rs` — RMCP tool handler (9 tools)
- `main.rs` — Entry point, logging setup

## Key Details

- Containers mount host workspace at `/workspace` and create `/artifacts` for outputs
- Container name auto-generated as `linux-build-<uuid8>` if not provided
- Deploy/SSH tools require board IP — pass as arg or configure with `--board-ip`
- No internal container state tracking — queries Docker directly
- Default Docker image: `stm32mp1-sdk` (configurable via `--docker-image`)
