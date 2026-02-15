# elf-analysis

ELF binary size analysis MCP server. Wraps Zephyr's `size_report` script for DWARF-based ROM/RAM breakdown with per-file attribution.

## Build

```bash
cargo build --release
```

## Configuration

Pass `--workspace` (Zephyr workspace root) at server start. The server derives `zephyr_base` from `{workspace}/zephyr`. Override with `--zephyr-base` if needed. Tools also accept `workspace_path` per-call.

## Tools

### `analyze_size`
Full ROM/RAM breakdown with per-file/module attribution. Returns a tree of size nodes.
- `elf_path` (required) — Path to ELF file
- `target` — "rom", "ram", or "all" (default: "all")
- `depth` — Tree depth limit (default: unlimited)
- `workspace_path` — Override workspace

### `compare_sizes`
Diff two ELFs to track size growth. Returns deltas with top increases/decreases.
- `elf_path_a` (required) — "Before" ELF
- `elf_path_b` (required) — "After" ELF
- `workspace_path` — Override workspace

### `top_consumers`
Quick "biggest files/symbols" view. Flattens the tree and sorts by size.
- `elf_path` (required) — Path to ELF file
- `target` (required) — "rom" or "ram"
- `limit` — Top N (default: 20)
- `level` — "file" (default) or "symbol"
- `workspace_path` — Override workspace

## Key Details

- Runs `python3 size_report` as a subprocess
- Requires Zephyr workspace with `zephyr/scripts/footprint/size_report`
- Python deps: `pyelftools`, `anytree`, `colorama`, `packaging`
- ELF must contain DWARF debug info for file-level attribution
- JSON output from size_report uses `identifier` for full paths, `name` for display — we map `identifier` to `SizeNode.name`
- `compare_sizes` flattens both trees to leaf level for accurate per-file deltas
