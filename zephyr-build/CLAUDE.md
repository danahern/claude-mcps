# zephyr-build

Zephyr RTOS build MCP server. Wraps `west build` for compiling Zephyr applications.

## Build

```bash
cargo build --release
```

## Configuration

Set `workspace_path` in config or pass per-call. Points to the directory containing `apps/` (e.g., the `zephyr-apps/` submodule).

## Tools

### Build
- `list_apps` — Scan workspace for Zephyr applications (directories with CMakeLists.txt). Reads `manifest.yml` for description, boards, libraries.
- `list_boards` — List supported boards. Fast mode returns common boards; `include_all=true` runs `west boards` (slow)
- `build` — Build an app for a target board. Supports `pristine=true` for clean builds and `background=true` for async
- `build_status` — Check progress of a background build
- `clean` — Remove build artifacts for an app
- `list_templates` — List available app templates with descriptions, default libraries, and generated files. Call before `create_app`.
- `create_app` — Create a new app from a template. Args: `name` (required), `template` (default "core"), `board`, `libraries`, `description`. Reads library manifests to generate OVERLAY_CONFIG lines.

### Test
- `run_tests` — Run Zephyr tests via twister. Supports `path` filter, `board`, `filter` (-k), `background` mode. Returns parsed summary.
- `test_status` — Check progress of a background test run. Returns summary when complete.
- `test_results` — Parse structured results from a completed run (by `test_id` or `results_dir`). Returns suites, test cases, and failures with logs.

## Key Details

- Runs `west build` as a subprocess with the Zephyr venv activated
- App path resolution: looks for `apps/<name>/` under the workspace path
- Build output goes to the app's `build/` directory by default
- Background builds return a `build_id` for polling via `build_status`
- Test tools run `python3 zephyr/scripts/twister` and parse `twister.json` output
- Test output goes to `.cache/twister/<test_id>/`
- Default test path is `lib/` under the apps parent directory
- Library manifests (`lib/<name>/manifest.yml`) declare `default_overlays` used by `create_app`
- App manifests (`apps/<name>/manifest.yml`) store description, boards, libraries, template metadata
