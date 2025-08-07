# Repository Guidelines

## Project Structure & Module Organization
- This crate lives under `crates/simulacrum`; `src/lib.rs` exposes the `Simulacrum` type and orchestrates epoch/transaction flows.
- Epoch-specific logic sits in `src/epoch_state.rs`.
- Storage adapters implement `SimulatorStore` under `src/store/`, with the in-memory implementation in `in_mem_store.rs`.
- Tests reside alongside code behind `#[cfg(test)]`; integration tests are not split out.

## Build, Test, and Development Commands
- Fast sanity check: `cargo check -p simulacrum`.
- Unit tests: `cargo test -p simulacrum` (use `SUI_SKIP_SIMTESTS=1` when running nextest in the workspace).
- Lint/format (workspace-level): `cargo fmt --all -- --check`, `cargo clippy -p simulacrum --all-features`, or `./scripts/lint.sh` before submitting.
- Docs for API review: `cargo doc -p simulacrum --no-deps --open`.

## Coding Style & Naming Conventions
- Rust 2024 edition; 4-space indent; keep line length readable (~100 cols).
- Follow Rust API patterns already in `lib.rs` (public methods documented, builder-style helpers).
- Use `tracing` for diagnostics, `anyhow::Result` for fallible flows in this crate.
- Traits and types: prefer descriptive names over abbreviations (`SimulatorStore`, `AdvanceEpochConfig`).
- Avoid unnecessary comments; document non-obvious invariants or protocol assumptions.

## Testing Guidelines
- Favor deterministic RNG seeds in tests when reproducibility matters; use `StdRng::seed_from_u64` as shown in doc examples.
- Add focused unit tests near the code they cover; prefer `cargo nextest run -p simulacrum --lib` for faster iterations.
- When touching epoch transitions or checkpoint logic, cover both successful paths and expected failures (e.g., deny-list or bridge toggles).

## Commit & Pull Request Guidelines
- Commit style mirrors history: concise, present-tense summaries with an optional scope prefix (e.g., `simulacrum: tighten epoch config`) and PR tooling appends `(#xxxx)`.
- PRs should include: purpose/context, notable behavior changes, test evidence (commands run), and any follow-up TODOs.
- Link issues where applicable; include screenshots/log snippets only when UI or output changes need illustration.

## Security & Configuration Tips
- Keep test-only knobs gated to test code; avoid widening visibility of cryptographic keys or configs.
- Preserve deterministic defaults in `ConfigBuilder` (temp dirs, single-validator setup) unless a test explicitly needs multi-validator behavior.
