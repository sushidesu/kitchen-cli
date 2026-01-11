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
```

## Usage

### Commands

```bash
# Print a greeting
kitchen hello
kitchen hello kitchen
```

## Development

```bash
cargo build
cargo run -- hello
```

## Roadmap

- Add more small utilities as top-level commands.
- Introduce categories only if the command list becomes too large.
