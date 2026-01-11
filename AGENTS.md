# Repository Guidelines

## Project Structure & Module Organization
- `src/` holds all Rust source code.
  - `src/main.rs` is the entry point.
  - `src/cli.rs` defines CLI parsing and command routing.
  - `src/commands/` contains feature modules (e.g., `hello.rs`).
- `Cargo.toml` and `Cargo.lock` define dependencies and versions.
- `target/` is build output and is ignored by Git.

## Build, Test, and Development Commands
- `cargo build` compiles the CLI binary.
- `cargo run -- <command>` runs the CLI locally (example: `cargo run -- hello`).
- `cargo test` runs unit tests (none yet, but use this when tests are added).

## Coding Style & Naming Conventions
- Follow standard Rust style (`rustfmt` defaults). Indentation is 4 spaces.
- Use `snake_case` for modules and functions, `CamelCase` for types, and `SCREAMING_SNAKE_CASE` for constants.
- Keep commands in `src/commands/` and expose them via `src/cli.rs`.

## Testing Guidelines
- No test framework is set up yet. When adding tests, prefer Rust’s built-in `#[test]`.
- Place tests next to the module they cover or under `tests/` for integration tests.
- Test names should describe behavior (example: `prints_hello_with_name`).

## Commit & Pull Request Guidelines
- No commit message convention is established yet. Use clear, imperative messages
  (example: `Add hello command`).
- PRs should describe the change, include relevant commands to verify, and link
  issues if applicable. Add screenshots only if the change affects output formatting.
