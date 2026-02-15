# elf-analysis

MCP server for ELF binary size analysis. Provides ROM/RAM usage breakdowns, per-file attribution, and binary size comparison — answering "how much flash/RAM am I using?", "where is memory going?", and "did this change bloat the binary?"

## Setup

### Prerequisites

- Rust toolchain
- Python 3 with: `pip install pyelftools anytree colorama packaging`
- Zephyr workspace (for `scripts/footprint/size_report`)

### Build

```bash
cd claude-mcps/elf-analysis
cargo build --release
```

### Register in `.mcp.json`

```json
"elf-analysis": {
    "command": "/path/to/claude-mcps/elf-analysis/target/release/elf-analysis",
    "args": ["--workspace", "/path/to/zephyr-workspace"]
}
```

## Usage

### Analyze size

Get full ROM/RAM breakdown for an ELF:

```
elf-analysis.analyze_size(elf_path="/path/to/zephyr.elf")
elf-analysis.analyze_size(elf_path="/path/to/zephyr.elf", target="rom", depth=2)
```

### Top consumers

Quick view of what's using the most memory:

```
elf-analysis.top_consumers(elf_path="/path/to/zephyr.elf", target="rom")
elf-analysis.top_consumers(elf_path="/path/to/zephyr.elf", target="ram", level="symbol", limit=10)
```

### Compare sizes

Diff two builds to find what grew:

```
elf-analysis.compare_sizes(elf_path_a="/path/to/before.elf", elf_path_b="/path/to/after.elf")
```

## Configuration

| Flag | Description | Default |
|------|-------------|---------|
| `--workspace` | Zephyr workspace root | None |
| `--zephyr-base` | Path to `zephyr/` directory | `{workspace}/zephyr` |
| `--log-level` | Logging level | `info` |
| `--log-file` | Log file path | stderr |

## Troubleshooting

### "size_report script not found"
Set `--workspace` to your Zephyr workspace root, or `--zephyr-base` to the directory containing `scripts/footprint/size_report`.

### "Missing Python dependencies"
Install the required packages: `pip install pyelftools anytree colorama packaging`

### Empty or missing results
The ELF file must contain DWARF debug info. Build with debug symbols enabled (default in Zephyr debug builds).
