# Implementation Plan: GRPC Simulate API - Compatibility Layer Gas Coin

## Background

SDK v2 uses `tx.gas` (Argument::GasCoin) to allow apps to access their entire SUI balance within a PTB. With the new address balance system, we need the compatibility layer to emulate this behavior.

**Key requirements from Slack discussion:**
1. When `tx.gas` is present, use the compatibility layer to create a coin from address balance
2. The compatibility coin should be **first** in the payment list (driving funds towards address balances for free tier benefits)
3. `tx.gas` is incompatible with explicit SUI reservations
4. GraphQL calls the GRPC simulate API, so this change handles both

## Current State

### How gas selection works (`simulate/mod.rs:select_gas()`)
1. Detects if `Argument::GasCoin` is used via `is_gas_coin_used()`
2. If gas coin is NOT used AND address balance is sufficient → use address balance
3. Otherwise → select gas coins from owned objects

### The compatibility layer mechanism
1. `FundsWithdrawalArg` with `from_compatibility_object: true` flag
2. At PTB execution, withdrawals with this flag are converted to `Coin<T>` via `coin::redeem_funds()`
3. The resulting coin can be used like any other coin in the PTB

## Implementation Plan

### Phase 0: Write Tests First

Before implementing any functionality, write tests for the following scenarios. Each test constructs a PTB that uses `Argument::GasCoin` to send X SUI (e.g., X = 1_000_000_000 MIST = 1 SUI) and verifies correct behavior.

**Test Location**: `crates/sui-rpc-api/src/grpc/v2/transaction_execution_service/simulate/tests.rs` (or appropriate test module)

#### Test Case 1: X satisfied entirely by coins, with AB funds available
```
Setup:
- Account has 5 SUI in coins
- Account has 3 SUI in address balance
- X = 1 SUI

PTB: SplitCoins(GasCoin, [X]) → TransferObjects([Result(0)], recipient)

Expected behavior (current): Uses coin for gas, GasCoin refers to the gas coin
Expected behavior (with compatibility layer): Should still work, uses coin
```

#### Test Case 2: X satisfied entirely by coins, no AB funds
```
Setup:
- Account has 5 SUI in coins
- Account has 0 SUI in address balance
- X = 1 SUI

PTB: SplitCoins(GasCoin, [X]) → TransferObjects([Result(0)], recipient)

Expected behavior: Uses coin for gas, GasCoin refers to the gas coin
```

#### Test Case 3: X satisfied entirely by address balance, with coins available
```
Setup:
- Account has 1 SUI in coins (enough for gas but not X)
- Account has 5 SUI in address balance
- X = 2 SUI

PTB: SplitCoins(GasCoin, [X]) → TransferObjects([Result(0)], recipient)

Expected behavior (with compatibility layer):
- Creates FundsWithdrawal for address balance
- GasCoin remapped to compatibility coin
- Gas paid from address balance
```

#### Test Case 4: X satisfied entirely by address balance, no coins
```
Setup:
- Account has 0 SUI in coins
- Account has 5 SUI in address balance
- X = 1 SUI

PTB: SplitCoins(GasCoin, [X]) → TransferObjects([Result(0)], recipient)

Expected behavior (with compatibility layer):
- Creates FundsWithdrawal for address balance
- GasCoin remapped to compatibility coin
- Gas paid from address balance
```

#### Test Case 5: X requires combined withdrawal from coins and address balance
```
Setup:
- Account has 2 SUI in coins
- Account has 2 SUI in address balance
- X = 3 SUI (requires both sources)

PTB: SplitCoins(GasCoin, [X]) → TransferObjects([Result(0)], recipient)

Expected behavior: TBD - this is an edge case that needs design decision:
- Option A: Error - tx.gas doesn't support combined sources
- Option B: Use largest available source
- Option C: Some merging strategy

Note: Per Slack, "tx.gas will be incompatible with explicit reservations of SUI"
which suggests we should NOT try to combine sources. Recommend Option A (error)
or use whichever single source can satisfy X.
```

#### Test Case 6: Insufficient funds even when combining all sources
```
Setup:
- Account has 1 SUI in coins
- Account has 1 SUI in address balance
- X = 5 SUI (more than total available)

PTB: SplitCoins(GasCoin, [X]) → TransferObjects([Result(0)], recipient)

Expected behavior: Error - insufficient funds
```

#### Additional Test Cases

**Test Case 7: tx.gas with explicit SUI reservation (should error)**
```
Setup:
- Account has 5 SUI in address balance
- X = 1 SUI
- PTB also has explicit FundsWithdrawal input for SUI

Expected behavior: Error - tx.gas incompatible with explicit SUI reservations
```

**Test Case 8: Sponsored transaction with tx.gas**
```
Setup:
- Sender has 0 SUI
- Sponsor has 5 SUI in address balance
- X = 1 SUI

PTB: SplitCoins(GasCoin, [X]) → TransferObjects([Result(0)], recipient)

Expected behavior: Sponsor's address balance used via WithdrawFrom::Sponsor
```

**Test Case 9: Protocol config disabled**
```
Setup:
- Account has 5 SUI in address balance only (no coins)
- X = 1 SUI
- convert_withdrawal_compatibility_ptb_arguments = false

Expected behavior: Should fall back to coin selection, which fails (no coins)
```

#### Test Implementation Instructions

1. Create test file with test harness that can:
   - Set up accounts with specific coin and address balance amounts
   - Construct PTBs that use `Argument::GasCoin`
   - Call the simulate API
   - Verify the resulting transaction structure and effects

2. Initially, tests for cases 3, 4, 5 (address balance scenarios) should **fail** since the compatibility layer isn't implemented yet.

3. Tests for cases 1, 2 (coin-only scenarios) should **pass** with existing behavior.

4. After implementing the compatibility layer, all tests should pass.

### Phase 1: Detect tx.gas usage and determine if compatibility layer is needed

**File: `crates/sui-rpc-api/src/grpc/v2/transaction_execution_service/simulate/mod.rs`**

In `select_gas()`, add a new branch when `gas_coin_used == true`:

```rust
// Existing check
let gas_coin_used = transaction
    .kind()
    .iter_commands()
    .any(Command::is_gas_coin_used);

// NEW: If gas coin is used and we have sufficient address balance,
// use the compatibility layer instead of coin selection
if gas_coin_used
    && protocol_config.convert_withdrawal_compatibility_ptb_arguments()
    && let Some(address_balance) = address_balance
    && address_balance >= budget
{
    // Check for explicit SUI reservations - incompatible with tx.gas
    let has_sui_reservations = /* check for existing SUI FundsWithdrawal inputs */;
    if has_sui_reservations {
        return Err(/* error: tx.gas incompatible with explicit SUI reservations */);
    }

    // Insert compatibility coin
    insert_compatibility_gas_coin(transaction, budget, current_epoch)?;
}
```

### Phase 2: Insert compatibility gas coin into PTB

**New function in `simulate/mod.rs`:**

```rust
fn insert_compatibility_gas_coin(
    transaction: &mut TransactionData,
    budget: u64,
    current_epoch: EpochId,
) -> Result<()> {
    let TransactionKind::ProgrammableTransaction(ptb) = transaction.kind_mut() else {
        // Should not happen - we only get here for PTBs
        return Err(/* error */);
    };

    // 1. Create FundsWithdrawal input for the gas budget
    let gas_withdrawal = FundsWithdrawalArg::balance_from_sender(
        budget,
        GAS::type_tag(), // SUI
    );

    // 2. Insert at the BEGINNING of inputs (first in payment list)
    let withdrawal_input_idx = 0u16;
    ptb.inputs.insert(0, CallArg::FundsWithdrawal(gas_withdrawal));

    // 3. Shift all existing Input references in commands
    shift_input_references(ptb, withdrawal_input_idx)?;

    // 4. Remap all Argument::GasCoin references to use the new input
    //    Note: The execution layer will convert this to Result(0, 0) after
    //    the compatibility conversion
    remap_gas_coin_to_input(ptb, withdrawal_input_idx)?;

    // 5. Set ValidDuring expiration for address balance usage
    *transaction.expiration_mut() = TransactionExpiration::ValidDuring {
        min_epoch: Some(current_epoch),
        max_epoch: Some(current_epoch.saturating_add(1)),
        ...
    };

    // 6. Clear gas payment to use address balance
    transaction.gas_data_mut().payment.clear();

    Ok(())
}
```

### Phase 3: Handle input index shifting and GasCoin remapping

When we insert a new input at position 0, all existing `Argument::Input(n)` references need to become `Argument::Input(n + 1)`.

**Critical finding**: The execution layer's `lift_result_indices()` function explicitly does **NOT** remap `Argument::GasCoin`:
```rust
// From sui-execution/latest/.../typing/translate.rs:1063
L::Argument::GasCoin => (),
```

The execution layer only remaps `Argument::Input(i)` references to `Argument::NestedResult(converted_idx, 0)` for compatibility inputs. This means:

1. PTB input 0 = FundsWithdrawal (compatibility)
2. Execution converts input 0 → Result(0, 0) = Coin<SUI>
3. But `Argument::GasCoin` references are **not** automatically remapped

**Solution**: The simulate API must replace all `Argument::GasCoin` references with `Argument::Input(0)` **before** the transaction is executed. Then the execution layer's existing remapping logic will handle converting `Input(0)` → `NestedResult(0, 0)`.

**Implementation:**

```rust
fn remap_gas_coin_to_input(ptb: &mut ProgrammableTransaction, input_idx: u16) {
    for command in &mut ptb.commands {
        for arg in command.arguments_mut() {
            if *arg == Argument::GasCoin {
                *arg = Argument::Input(input_idx);
            }
        }
    }
}
```

This approach:
- Keeps changes localized to the simulate API
- Leverages existing execution layer compatibility conversion logic
- The resulting coin behaves exactly like the gas coin would have

### Phase 4: Resolve integration points

**Files to modify:**
1. `crates/sui-rpc-api/src/grpc/v2/transaction_execution_service/simulate/mod.rs`
   - Modify `select_gas()` to detect and handle compatibility gas coin case
   - Add `insert_compatibility_gas_coin()` helper
   - Add input index shifting logic
   - Add GasCoin → Input remapping logic

2. `crates/sui-rpc-api/src/grpc/v2/transaction_execution_service/simulate/resolve/mod.rs`
   - Potentially need to mark the compatibility input appropriately during resolution

### Edge Cases

#### 1. tx.gas with explicit SUI reservations
**Behavior**: Return error
**Reason**: Per Slack, these are incompatible. Users switching to reservations should also migrate away from tx.gas.

```rust
// Check for existing SUI FundsWithdrawal inputs
let has_sui_reservations = ptb.inputs.iter().any(|input| {
    matches!(input, CallArg::FundsWithdrawal(w)
        if GAS::is_gas_type(&w.type_arg.get_balance_type_param().unwrap_or_default()))
});
if has_sui_reservations {
    return Err(RpcError::new(
        tonic::Code::InvalidArgument,
        "Argument::GasCoin cannot be used with explicit SUI balance reservations. \
         Migrate to using the reservation directly instead of tx.gas."
    ));
}
```

#### 2. Sponsored transactions
**Behavior**: The gas owner (sponsor) pays for gas from their address balance
**Implementation**: `FundsWithdrawalArg` should use `WithdrawFrom::Sponsor` when gas owner != sender

```rust
let withdraw_from = if transaction.gas_data().owner != transaction.sender() {
    WithdrawFrom::Sponsor
} else {
    WithdrawFrom::Sender
};
```

#### 3. Protocol config gating
**Behavior**: Only use compatibility layer when `convert_withdrawal_compatibility_ptb_arguments()` is enabled
**Implementation**: Check this flag before inserting compatibility coin. If disabled, fall back to existing coin selection.

#### 4. Budget estimation with compatibility coin
**Behavior**: When estimating budget, we need to simulate with the compatibility coin approach
**Implementation**: The existing budget estimation flow should work - we just need to ensure the compatibility coin is included when re-simulating for budget estimation.

#### 5. Insufficient address balance
**Behavior**: Fall back to coin selection (existing behavior)
**Implementation**: The existing check `address_balance >= budget` handles this.

#### 6. Non-PTB transactions
**Behavior**: Don't use compatibility layer (only PTBs support GasCoin argument)
**Implementation**: The existing `is_gas_coin_used()` check handles this since non-PTB transactions won't have GasCoin usage.

#### 7. Gas coin used multiple times
**Behavior**: All references should point to the same compatibility coin
**Implementation**: Single compatibility input, all GasCoin references remapped to it.

### Testing Plan

**Important**: Tests should be written BEFORE implementation (see Phase 0).

1. **Primary test cases (Phase 0)** - Write these first:
   - Test Cases 1-6: Various combinations of coins and address balance availability
   - Test Case 7: tx.gas with explicit SUI reservation (error case)
   - Test Case 8: Sponsored transaction with tx.gas
   - Test Case 9: Protocol config disabled

2. **Unit tests for helper functions** (after implementation begins):
   - `shift_input_references()`: Verify Input(0) becomes Input(1), etc.
   - `remap_gas_coin_to_input()`: Verify GasCoin becomes Input(0)
   - `insert_compatibility_gas_coin()`: Verify correct FundsWithdrawal structure

3. **E2E tests** (`sui-e2e-tests`):
   - Full simulate → execute flow with compatibility coin
   - Verify funds flow to/from address balance correctly
   - Verify returned balance changes are accurate

### Rollout Considerations

1. **Testnet first**: The SDK changes will be released alongside testnet rollout
2. **Wallet upgrade**: Wallets that upgraded to SDK v2 before this change will need to upgrade again (dep bump only)
3. **Backward compatibility**: Old SDK versions (pre-v2) don't use this path, so no impact

### Open Questions

1. **Result value access**: When a command uses the result of the compatibility coin (e.g., splits it), how does this interact with the remapping? Need to verify the execution layer handles this correctly.

2. **Dry run consistency**: Ensure `dry_run_transaction_block` (JSON-RPC) and `simulate_transaction` (GRPC) behave consistently. Currently JSON-RPC expects SDK to construct the compatibility coin directly.

3. **Mock gas coin behavior**: The simulate API has `allow_mock_gas_coin` parameter. How does this interact with the compatibility layer? Should we use a mock compatibility coin for budget estimation?

## Critical Implementation Detail: Passing withdrawal_compatibility_inputs

### The Challenge

The execution layer expects a `withdrawal_compatibility_inputs: Option<Vec<bool>>` parameter to know which PTB inputs should be converted to coins. Currently:

1. The `TransactionExecutor::simulate_transaction` interface only takes `TransactionData`
2. The execution engine always passes `None` for `withdrawal_compatibility_inputs`
3. There's no mechanism to communicate which inputs are compatibility inputs through the existing interface

### Analysis

The `withdrawal_compatibility_inputs` flows through:
```
TransactionExecutor::simulate_transaction(TransactionData)
  → execution_engine.rs execution_loop()
    → programmable_transactions::execution::execute()  // passes None
      → static_programmable_transactions::execute()
        → loading::translate::transaction()  // uses withdrawal_compatibility_inputs here
```

### Proposed Solution

**Option A**: Modify the `TransactionExecutor` trait to accept `withdrawal_compatibility_inputs`

```rust
// In sui-types/src/transaction_executor.rs
fn simulate_transaction(
    &self,
    transaction: TransactionData,
    checks: TransactionChecks,
    allow_mock_gas_coin: bool,
    withdrawal_compatibility_inputs: Option<Vec<bool>>,  // NEW
) -> Result<SimulateTransactionResult, SuiError>;
```

This is the cleanest approach but requires changes throughout the stack.

**Option B**: Encode compatibility info in a sidecar data structure

Add a new field to `TransactionData` or use a wrapper type that carries the compatibility inputs alongside the transaction.

**Option C**: Auto-detect compatibility inputs in execution layer

Have the execution layer automatically mark FundsWithdrawal inputs created by gas selection as compatibility inputs. This could be done by:
- Using a special marker in the `FundsWithdrawalArg`
- Detecting inputs at index 0 that are FundsWithdrawal for SUI

**Recommendation**: Start with Option A as it's the most explicit. If that's too invasive, fall back to Option C with auto-detection.

### Files Requiring Changes (Updated)

1. **`crates/sui-types/src/transaction_executor.rs`**
   - Modify trait to accept `withdrawal_compatibility_inputs`

2. **`crates/sui-rpc-api/src/grpc/v2/transaction_execution_service/simulate/mod.rs`**
   - Build `withdrawal_compatibility_inputs` Vec when inserting compatibility coin
   - Pass it to executor

3. **Execution layer** (if using Option C instead)
   - `sui-execution/*/sui-adapter/src/execution_engine.rs`
   - Detect and pass `withdrawal_compatibility_inputs` based on transaction structure
