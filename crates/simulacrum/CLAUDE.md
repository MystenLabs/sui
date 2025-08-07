# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Essential Development Commands

### Building
```bash
# Build the simulacrum crate
cargo build -p simulacrum

# Check code without building (preferred for faster iteration)
cargo check -p simulacrum
```

### Testing
```bash
# Run tests using nextest (preferred)
cargo nextest run -p simulacrum

# Run only library tests for faster feedback
cargo nextest run --lib -p simulacrum

# Run with standard cargo test
cargo test -p simulacrum

# Skip simulation tests for deterministic results
SUI_SKIP_SIMTESTS=1 cargo nextest run -p simulacrum
```

### Linting and Formatting
```bash
# Format code
cargo fmt --all -- --check

# Run clippy linting (MUST run after development)
cargo xclippy
```

## High-Level Architecture

### Purpose
The `simulacrum` crate provides a simulated Sui blockchain environment for testing. It creates a "likeness" of the Sui blockchain that can be manually controlled - time doesn't advance and checkpoints aren't formed unless explicitly requested.

### Core Components

#### 1. **Simulacrum Struct** (`src/lib.rs`)
The main entry point that orchestrates the simulation:
- Manages transaction execution without consensus
- Controls time advancement manually via `advance_clock()`
- Creates checkpoints on demand via `create_checkpoint()`
- Handles epoch transitions via `advance_epoch()`
- Provides deterministic chain creation with seeded RNG

#### 2. **Store Trait & Implementation** (`src/store/`)
- **SimulatorStore trait**: Defines storage interface for checkpoints, transactions, objects
- **InMemoryStore**: In-memory implementation for testing
- **KeyStore**: Manages validator and account keypairs
- Tracks object versions, ownership, and lifecycle

#### 3. **EpochState** (`src/epoch_state.rs`)
Manages epoch-specific execution state:
- Holds committee configuration
- Manages protocol configuration
- Executes transactions through Sui's execution pipeline
- Handles gas calculations and transaction checks
- Integrates with Move VM for smart contract execution

### Key Workflows

#### Transaction Execution Flow
1. Transaction validation (signatures, ownership, gas)
2. Input object loading from store
3. Move VM execution via `sui_execution::Executor`
4. Effects generation and state updates
5. Enqueue for checkpoint inclusion

#### Checkpoint Creation
1. Collect enqueued transactions since last checkpoint
2. Build checkpoint with MockCheckpointBuilder
3. Sign with validator keys from KeyStore
4. Update store with checkpoint and contents

#### Epoch Advancement
1. Create EndOfEpoch transaction
2. Update committee if needed
3. Build final checkpoint of epoch
4. Transition to new EpochState

### Testing Capabilities

- **Deterministic Testing**: Use seeded RNG for reproducible chain states
- **Funded Accounts**: Create accounts with gas via `funded_account()`
- **Gas Management**: Request gas for addresses via `request_gas()`
- **Time Control**: Manually advance clock for time-based testing
- **State Inspection**: Direct access to objects, transactions, and effects

### Important Development Notes

1. **Incomplete Features** (marked as TODO in code):
   - Some `ReadStore` trait methods not implemented
   - Child object reads don't support bounded reads
   - Transaction input loading out-of-sync with production
   - Protocol version updates during epoch changes incomplete

2. **Testing Requirements**:
   - All tests must pass - never disable tests
   - Use `#[tokio::test]` for async tests
   - Set compilation/test timeouts to 10+ minutes due to codebase size

3. **Performance Tips**:
   - Use `-p simulacrum` to test only this crate
   - Use `cargo check` instead of `cargo build` for faster iteration
   - Use `--lib` flag to skip integration tests when appropriate

4. **Critical Post-Development Steps**:
   - Always run `cargo xclippy` before committing
   - Ensure all tests pass with `cargo nextest run -p simulacrum`