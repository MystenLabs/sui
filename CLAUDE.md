# CLAUDE.md

## Crate-specific CLAUDE.md files
When a sub-crate's CLAUDE.md conflicts with this file, the sub-crate's instructions win.

## Individual Preferences
Individual preferences supersede and extend project preferences:
- @CLAUDE.local.md if present.

## Essential Development Commands

### License comments

All new files must start with the following license in comments at the top of the file:

    Copyright (c) Mysten Labs, Inc.
    SPDX-License-Identifier: Apache-2.0

### Building and Installation

```bash
# Build a specific crate. Generally don't need to do release build.
cargo build -p sui-core

# Check code without building (preferred)
cargo check
```

### Testing

```bash
# Run e2e tests. simtests must be run with `cargo simtest` to avoid false negatives
cargo simtest -p sui-e2e-tests

# Run Rust unittests. skip simulation tests as they may cause false negatives with `cargo nextest`
SUI_SKIP_SIMTESTS=1 cargo nextest run
```

**Important Notes for Testing:**
- When compiling or running tests in this repository, set timeout limits to at least 10 minutes due to the large codebase size
- For faster iteration, use -p to select only the most relevant packages for testing. Use multiple `-p` flags if necessary, e.g. `cargo nextest run -p sui-types -p sui-core`
- Use `cargo nextest run --lib` to run only library tests and skip integration tests for faster feedback
- Use a scoped `cargo insta test` for the relevant package when snapshots are affected. Inspect the generated snapshot diffs. If they match the intended changes, update them with `cargo insta accept`. Do not accept unrelated snapshot changes.
- Consult crate-specific CLAUDE.md files for instructions on which tests to run, when changing files in those crates

### Linting and Formatting

```bash
# Formats & lints all Rust & Move, run before commit:
./scripts/lint.sh

# Alternatively, run individual lints on specific crates (much faster than linting the whole repo):
# For crates in `crates/`: cd into the crate directory and run:
cargo xclippy
# For crates in `external-crates/`: cd into the crate directory and run:
cargo move-clippy
# For formatting:
cargo fmt --all -- --check
```

`cargo xclippy` does not recognize the `-p` option - cd into the crate directory instead.

## High-Level Architecture

### Core Components Structure

```
sui/
├── crates/                             # Main Rust crates
│   ├── sui-core/                       # Core blockchain logic
│   ├── sui-node/                       # Validator node implementation
│   ├── sui-framework/                  # Move system packages & stdlib
│   ├── sui-types/                      # Core type definitions
│   ├── sui-json-rpc/                   # JSON-RPC API server
│   ├── sui-indexer-alt-graphql/        # GraphQL API server
│   └── sui-indexer-alt/                # Blockchain data indexer
├── consensus/                          # Consensus mechanism (Mysticeti)
├── sui-execution/                      # Move execution layer with versions
├── dapps/                              # Frontend applications
└── external-crates/                    # Move compiler and VM
```

### Key Architectural Patterns

1. **Authority System**: Sui uses a set of validators (authorities) that process transactions in parallel. Each authority maintains its own state and participates in Mysticeti consensus.

2. **Data Model**: Sui supports an object data model where each object has a unique ID and version. Accounts can also own balances.

3. **Transaction Flow**:
   - User → Fullnode → Validators
   - All user transactions require consensus voting and commit before execution.
   - Pre and post-consensus fastpath executions have been removed. Surviving mentions of "fastpath" either refer to consensus transaction-voting logic, or should be removed. There is no longer a separate execution path called fastpath.

4. **Storage Layer**:
   - Uses RocksDB for persistent storage
   - Separate stores for permanent, per-epoch, checkpoint, consensus and indexing data

5. **Execution Pipeline**:
   - Consensus output → Execution → Effects commitment
   - Move VM executes smart contracts with gas metering
   - Parallel execution for non-conflicting transactions

### Test-Only Code

Use `#[cfg(test)]` for test-only code used within the same crate. Use `#[cfg(feature = "testing")]` for test-only code that must be callable cross-crate. For the `testing` feature: define `testing = []` in the crate's `Cargo.toml`, and callers must propagate it via `features = ["testing"]` in their dependency declaration.

### Critical Development Notes
1. **Testing Requirements**:
   - Always run tests before submitting changes
   - Framework changes require snapshot updates
2. **Protocol Config Changes**:
   - When modifying `crates/sui-protocol-config/src/lib.rs`, always invoke `/protocol-config` to verify changes are safe. Incorrect changes can break network consensus.
3. **Raising a PR**:
   - When opening or updating a PR in this repo, always invoke the `/send-pr` skill.
4. **CRITICAL - Final Development Steps**:
   - **ALWAYS run `cargo xclippy` after finishing development** to ensure code passes all linting checks
   - **NEVER disable or ignore tests** - all tests must pass and be enabled
   - **NEVER use `#[allow(dead_code)]`, `#[allow(unused)]`, or any other linting suppressions** - fix the underlying issues instead
   - **All unit tests must work properly** - use `#[tokio::test]` for async tests, not `#[test]`

### Comment Writing Guidelines

**Do NOT comment the obvious** - comments should not simply repeat what the code does.
**When to comment**:
- Non-obvious algorithms or business logic
- Temporary exclusions, timeouts, or thresholds and their reasoning
- Complex calculations where the "why" isn't immediately clear
- Subtle race conditions or threading considerations
- Assumptions about external state or preconditions

**When NOT to comment**:
- Simple variable assignments
- Standard library usage
- Self-descriptive function calls
- Basic control flow (if/for/while)
