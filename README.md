# rustop

An htop-like process viewer for the terminal, written in Rust using [ratatui](https://ratatui.rs/) and [sysinfo](https://crates.io/crates/sysinfo).

## Features

- CPU, memory, swap, and load meters at a glance
- Sortable process table (CPU, memory, PID, name) with ascending/descending toggle
- Keyboard navigation with a selectable row
- Expandable command-line panel for the highlighted process
- Graceful terminal restoration on exit or panic

## Install

### From a release archive

Download the archive for your platform from the [Releases page](https://github.com/wingnut128/rustop/releases), extract, and move the binary onto your `PATH`:

```sh
tar -xzf rustop-vX.Y.Z-<target>.tar.gz
sudo mv rustop-vX.Y.Z-<target>/rustop /usr/local/bin/
```

Prebuilt targets:

| Platform              | Target triple                  |
| --------------------- | ------------------------------ |
| Linux x86_64          | `x86_64-unknown-linux-gnu`     |
| Linux ARM64           | `aarch64-unknown-linux-gnu`    |
| macOS Apple Silicon   | `aarch64-apple-darwin`         |

Each archive ships with a `.sha256` file — verify before installing:

```sh
shasum -a 256 -c rustop-vX.Y.Z-<target>.tar.gz.sha256
```

### From source

Requires Rust 1.88+ (2024 edition) and [just](https://crates.io/crates/just).

```sh
cargo install just
git clone https://github.com/wingnut128/rustop
cd rustop
just build
./target/release/rustop
```

## Usage

```sh
rustop [--interval <milliseconds>]
```

The refresh interval defaults to one second. For example, use `rustop --interval 500`
to refresh twice per second. The interval must be a positive number of milliseconds.

### Controls

| Key            | Action                       |
| -------------- | ---------------------------- |
| `1`            | Sort by CPU                  |
| `2`            | Sort by memory               |
| `3`            | Sort by PID                  |
| `4`            | Sort by name                 |
| `↑` / `k`      | Move selection up            |
| `↓` / `j`      | Move selection down          |
| `g` / Home     | Select first process         |
| `G` / End      | Select last process          |
| PageUp/PageDown| Move one table page          |
| `/`            | Filter by process name       |
| `n` / `N`      | Next/previous filter match   |
| `Space`        | Pause/resume collection      |
| `Enter`        | Expand/collapse command line |
| `Esc`          | Clear filter, or quit        |
| `q`            | Quit                         |

Pressing the same sort key twice flips the sort direction.
The process title shows filtered and total counts. The table always shows one-line
process names; the selected process's complete command is available in its detail panel.

## Development

```sh
just run      # run in dev
just test     # run tests
just lint     # clippy with -D warnings
just fmt      # rustfmt
just check    # cargo check
```

## License

MIT — see [LICENSE](LICENSE).
