# CLAUDE.md

HTOP-like process viewer built in Rust using ratatui and sysinfo.

## Runtime & Tooling
- **Rust**: 2024 edition (requires 1.88+)
- **Package manager**: cargo
- **Build**: `just build` (release) or `just build-debug` (debug)
- **Lint**: `just lint` (clippy with -D warnings)
- **Format**: `just fmt` (rustfmt)
- **Run**: `just run`

## Commands
- **Build**: `just build`
- **Run in dev**: `just run`
- **Lint**: `just lint`
- **Format**: `just fmt`
- **Test**: `just test`
- **Check**: `just check`

## Controls
- `1` = Sort by CPU
- `2` = Sort by MEM
- `3` = Sort by PID
- `4` = Sort by Name
- `↑/↓` or `j/k` = Navigate
- `Enter` = Expand/Collapse command
- `q` or `Esc` = Quit