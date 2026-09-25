# CLAUDE.md

## Crate-specific CLAUDE.md files
When a sub-crate's CLAUDE.md conflicts with this file, the sub-crate's instructions win.

## Individual Preferences
Individual preferences supersede and extend project preferences:
- @CLAUDE.local.md if present.

## Essential Development Commands

### License comments

All applicable source code files must start with the following license in comments at the top of the file:

    Copyright (c) Mysten Labs, Inc.
    SPDX-License-Identifier: Apache-2.0

### Building and Installation

```bash
# Build a specific crate. Generally don't need to do release build.
cargo build -p sui-core

# Check code without code generation or linking (preferred)
cargo check
```

### Testing

```bash
# Run e2e tests. simtests must be run with `cargo simtest` to avoid false negatives
cargo simtest -p sui-e2e-tests

# Run Rust unittests. skip simulation tests as they may cause false negatives with `cargo nextest`
SUI_SKIP_SIMTESTS=1 cargo nextest run -p <crate-name>
```

**Important Notes for Testing:**
- When compiling or running tests in this repository, set timeout limits to at least 10 minutes due to the large codebase size
- For faster iteration, use -p to select only the most relevant packages for testing. Use multiple `-p` flags if necessary, e.g. `cargo nextest run -p sui-types -p sui-core`
- Use `cargo nextest run --lib` to run only library tests and skip integration tests for faster feedback
- Use a scoped `cargo insta test` for the relevant package when snapshots are affected. Inspect the generated snapshot diffs. If they match the intended changes, update them with `cargo insta accept`. Do not accept unrelated snapshot changes.
- Consult crate-specific CLAUDE.md files for instructions on which tests to run, when changing files in those crates

### Linting and Formatting

```bash
# Formats & lints all Rust & Move (can be slow).
./scripts/lint.sh

# For formatting:
cargo fmt --all

# Lint a single crate in `crates/`, `consensus/`, `sui-execution/`:
cargo xclippy -p <crate-name>

# Linting all crates in `external-crates/`: cd into the crate directory and run:
cargo move-clippy
```

## High-Level Architecture

### Core Components Structure

```
sui/
├── crates/                             # Main Rust crates
│   ├── sui-core/                       # Core blockchain logic
│   ├── sui-node/                       # Validator node implementation
│   ├── sui-framework/                  # Move system packages & stdlib
│   ├── sui-types/                      # Core type definitions
│   ├── sui-indexer-alt-jsonrpc/        # JSON-RPC API server
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
   - Pre and post-consensus fastpath executions have been removed. Surviving mentions of "fastpath" refer to consensus transaction-voting logic, owned object logic, or should be reworded or removed. There is no longer a separate execution path called fastpath.

4. **Storage Layer**:
   - Uses RocksDB or Tidehunter for persistent storage on Sui nodes.
   - Separate stores for permanent, per-epoch, checkpoint, consensus and indexing data

5. **Execution Pipeline**:
   - Consensus output → Execution → Effects commitment
   - Move VM executes smart contracts with gas metering
   - Parallel execution for non-conflicting transactions

## Development Notes

### Build flags

Sui binaries like sui-node built with `release` profile have `panic=abort` enabled.

### Test-Only Code

Use `#[cfg(test)]` for test-only code used within the same crate. Use `#[cfg(feature = "testing")]` for test-only code that must be callable cross-crate. For the `testing` feature: define `testing = []` in the crate's `Cargo.toml`, and callers must propagate it via `features = ["testing"]` in their dependency declaration.

Use `#[tokio::test]` for async tests, not `#[test]`.

### Protocol Config Changes:

When modifying `crates/sui-protocol-config/src/lib.rs`, always invoke `/protocol-config` to verify changes are safe. Incorrect changes can break network consensus.

### On-Wire Data Structure Changes (two-PR rule):

Anything reachable from `CheckpointData` in `crates/sui-types/src/full_checkpoint_content.rs`
(`CertifiedCheckpointSummary`, `CheckpointContents`, `CheckpointTransaction`, and everything they
contain: `TransactionData`, `TransactionEffects`, `Object`, events, etc.) is BCS-serialized and
decoded by external consumers (Rust SDK, TypeScript SDK, indexers, GraphQL, internal services)
that ship on their own schedule. BCS cannot skip an unknown enum variant, so a variant that
appears on chain before consumers have the new type definition makes them panic.

Any change that alters `crates/sui-types/tests/snapshots/format__sui.yaml.snap` (new enum variant,
new field, changed layout) MUST be split into two PRs:

1. **PR 1: add the type.** Add the variant/field and the protocol feature flag that gates producing
   it. The flag must NOT be enabled in any protocol version, or only under the devnet guard
   (`chain != Chain::Mainnet && chain != Chain::Testnet`). Nothing may produce the new shape on
   testnet or mainnet.
2. **PR 2: enable it.** Flip the flag in a later protocol version only after PR 1 has shipped in a
   Sui release AND the downstream decoders (sui-rust-sdk `sui-sdk-types`, TypeScript SDK, indexer
   and GraphQL pipelines) have released with the new type. Leave at least one release cycle
   between PR 1 and PR 2.

Never add a new on-wire variant and enable it on testnet/mainnet in the same PR.
Note in the PR description which downstream consumers were updated.

### Raising a PR:

When opening or updating a PR in this repo, always invoke the `/send-pr` skill.

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
