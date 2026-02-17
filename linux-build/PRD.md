# PRD: linux-build MCP Server

## Purpose

Docker-based cross-compilation and deployment server for Linux targets. Wraps the Docker CLI and SSH/SCP to enable AI-assisted Linux application development on embedded boards (STM32MP1 Cortex-A7, etc.) without the developer managing containers or SSH sessions manually.

## Technology Stack

| Component | Choice | Rationale |
|-----------|--------|-----------|
| Language | Rust | Consistent with other MCP servers |
| MCP SDK | rmcp 0.3.2 | Official Rust MCP SDK |
| Container | Docker CLI | Simpler than Docker API, no additional dependencies |
| Deployment | SSH/SCP | Standard Linux board access |
| Async | tokio 1 | Async subprocess execution |

## Tools (9)

| Tool | Args | Returns |
|------|------|---------|
| `start_container` | name?, image?, workspace_dir? | Container name and ID |
| `stop_container` | container | Confirmation |
| `container_status` | container | Running/exited/not found |
| `run_command` | container, command, workdir? | stdout, stderr, exit code |
| `build` | container, command?, workdir? | Build output with success/fail |
| `list_artifacts` | container, container_path? | File listing |
| `collect_artifacts` | container, container_path?, host_path | Copy confirmation |
| `deploy` | file_path, remote_path?, board_ip? | Deploy confirmation |
| `ssh_command` | command, board_ip? | stdout, stderr, exit code |

### Container Lifecycle

- `start_container`: Runs `docker run -d` with workspace volume mount. Auto-generates container name if not provided. Container runs `sleep infinity` to stay alive.
- `stop_container`: Runs `docker stop` then `docker rm -f`.
- `container_status`: Runs `docker inspect` to check state.

### Build Operations

- `run_command`: Generic `docker exec` wrapper for arbitrary commands.
- `build`: Convenience wrapper that runs a build command (default: `make`) in `/workspace`.
- `list_artifacts`: Lists files in the container's `/artifacts` directory.

### Deployment

- `collect_artifacts`: `docker cp` from container to host.
- `deploy`: `scp` file to board. Requires board IP (from arg or `--board-ip`).
- `ssh_command`: Execute command on board via SSH.

## Architecture

```
┌──────────────────────────────┐
│  MCP Tool Layer (9 tools)    │
├──────────────────────────────┤
│  docker_client.rs            │
│  Docker CLI wrapper          │
│  start/stop/exec/cp          │
│  SCP/SSH for deployment      │
├──────────────────────────────┤
│  Docker CLI (subprocess)     │
│  SSH/SCP (subprocess)        │
└──────────────────────────────┘
```

### Workflow

```
start_container → build → list_artifacts → collect_artifacts → deploy → ssh_command
```

## Key Design Decisions

1. **Docker CLI over Docker API**: Using `docker` CLI subprocess avoids pulling in Docker SDK dependencies and works identically to manual usage. The Docker socket isn't needed.

2. **Sleep-infinity pattern**: Containers run `sleep infinity` and stay alive for repeated `docker exec` calls, rather than one-shot builds. This supports iterative development.

3. **Host-side SCP/SSH**: Deployment runs from the host machine, not from inside the container. This avoids needing SSH keys inside the container and uses the developer's existing SSH configuration.

4. **No container tracking**: Unlike build servers with background builds, this server doesn't maintain internal state about containers. Docker itself tracks container state — `container_status` queries Docker directly.

5. **Generic build command**: The `build` tool takes a freeform command string rather than assuming Make/CMake/Buildroot. This supports any build system the Docker image contains.

## Testing

**13 tests** — all pass without Docker or SSH:

| Category | Count | Description |
|----------|-------|-------------|
| Config | 4 | Args parsing, defaults, from_args |
| Handler | 3 | Construction, custom config, server info |
| Validation | 4 | Missing board IP, missing file, error formatting |
| Docker | 2 | Container state display, nonexistent container |

```bash
cd claude-mcps/linux-build && cargo test
```
