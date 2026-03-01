# kitchen

An ever-growing Rust CLI that collects small, practical tools in one place.
Commands live at the top level so you can grow the toolbox without extra nesting.
Think of it as a personal kitchen drawer for quick utilities.

## Installation

Install from the GitHub repo with Cargo:

```bash
cargo install --git https://github.com/sushidesu/kitchen-cli
```

Or pin it globally via [mise](https://mise.jdx.dev/):

```bash
mise use -g cargo:sushidesu/kitchen-cli
```

## Quick Start

```bash
kitchen hello
kitchen hello kitchen
kitchen repo
```

## Usage

### Commands

```bash
# Print a greeting
kitchen hello
kitchen hello kitchen

# Incrementally search git repositories
kitchen repo
kitchen repo ~/dev
kitchen repo ~/dev ~/work
```

`kitchen repo` scans for git repositories under the given paths and starts an interactive
incremental selector in the terminal. It prints the selected repository path to stdout.
The selector supports real-time filtering with keyboard input.

Interactive keys:

- Type text: refine candidates immediately (fuzzy match)
- `Backspace`: delete one character from query
- `Up` / `Down`: move selection
- `Enter`: select and print path
- `Esc` / `Ctrl-C`: cancel

Search roots are resolved in this order:

1. Positional `PATH` arguments (`kitchen repo [path ...]`)
2. `~/.config/kitchen/config.toml` (`[repo].roots`)
3. Current directory (fallback when roots are empty)

Config example:

```toml
[repo]
roots = ["/Users/you/dev", "/Users/you/work"]
```

## Development

```bash
cargo build
cargo run -- hello
```

## Roadmap

- Add more small utilities as top-level commands.
- Introduce categories only if the command list becomes too large.
