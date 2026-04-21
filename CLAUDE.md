# CLAUDE.md

HTOP-like process viewer built in Rust using ratatui and sysinfo.

## Runtime & Tooling
- **Rust**: 2024 edition (requires 1.88+)
- **Package manager**: cargo
- **Build**: `cargo make build` (release) or `cargo make build-debug` (debug)
- **Lint**: `cargo make lint` (clippy with -D warnings)
- **Format**: `cargo make fmt` (rustfmt)
- **Run**: `cargo make run`

## Commands
- **Build**: `cargo make build`
- **Run in dev**: `cargo make run`
- **Lint**: `cargo make lint`
- **Format**: `cargo make fmt`
- **Test**: `cargo make test`
- **Check**: `cargo make check`

## Controls
- `1` = Sort by CPU
- `2` = Sort by MEM
- `3` = Sort by PID
- `4` = Sort by Name
- `↑/↓` or `j/k` = Navigate
- `Enter` = Expand/Collapse command
- `q` or `Esc` = Quit