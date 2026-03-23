# Implementation Plan: GRPC Simulate API - Gas Selection with Address Balance

## Background

The simulate API needs to select gas payment in a way that:
1. Maximizes smashing of coins into address balance (for free tier benefits)
2. Supports pure address balance payment when no coins are available
3. Gives users access to their full SUI balance via GasCoin when they use it

## Gas Payment Strategy

The gas payment strategy depends on what funds the sender has available AND whether `Argument::GasCoin` is used:

### When Argument::GasCoin IS used (user needs access to balance via tx.gas)

| Has AB | Has Coins | Strategy |
|--------|-----------|----------|
| Yes    | Yes       | Coin reservation FIRST (smashes coins into AB, user accesses combined) |
| Yes    | No        | Pure AB payment (empty `gas_data.payment` + expiration) |
| No     | Yes       | Traditional coin gas payment |
| No     | No        | Error: insufficient funds |

### When Argument::GasCoin is NOT used

| Has AB | Has Coins | Strategy |
|--------|-----------|----------|
| Yes    | Yes       | Prefer AB if sufficient, else coins (no reservation needed) |
| Yes    | No        | Pure AB payment |
| No     | Yes       | Traditional coin gas payment |
| No     | No        | Error: insufficient funds |

**Key insight**: When GasCoin is used, always use coin reservation if AB exists (to give user access to combined balance). When GasCoin is NOT used, prefer the simpler payment method.

**Note**: Coin reservations cannot be used in sponsored transactions.

## Implementation

### Location

`crates/sui-rpc-api/src/grpc/v2/transaction_execution_service/simulate/mod.rs`

### Updated `select_gas()` logic

```rust
fn select_gas(
    transaction: &mut TransactionData,
    address_balance: Option<u64>,
    owned_coins: &[ObjectRef],
    budget: u64,
    current_epoch: EpochId,
) -> Result<()> {
    let has_ab = address_balance.map_or(false, |ab| ab > 0);
    let has_coins = !owned_coins.is_empty();

    match (has_ab, has_coins) {
        (true, true) => {
            // Has both AB and coins: put coin reservation FIRST to smash coins into AB
            insert_coin_reservation_first(transaction, budget, current_epoch, owned_coins)?;
        }
        (true, false) => {
            // Has AB but no coins: use pure AB payment
            use_pure_ab_payment(transaction, current_epoch)?;
        }
        (false, true) => {
            // Has coins but no AB: traditional coin selection
            select_gas_coins(transaction, owned_coins, budget)?;
        }
        (false, false) => {
            // No funds available
            return Err(/* insufficient funds error */);
        }
    }

    Ok(())
}
```

### Helper: Coin reservation first (smashes coins into AB)

```rust
fn insert_coin_reservation_first(
    transaction: &mut TransactionData,
    budget: u64,
    current_epoch: EpochId,
    owned_coins: &[ObjectRef],
) -> Result<()> {
    // 1. Create coin reservation for the budget amount
    let reservation = create_sui_coin_reservation(budget, current_epoch);

    // 2. Build payment list: reservation FIRST, then coins
    let mut payment = vec![reservation];
    payment.extend(owned_coins.iter().cloned());
    transaction.gas_data_mut().payment = payment;

    // 3. Set ValidDuring expiration (required for AB usage)
    set_valid_during_expiration(transaction, current_epoch);

    Ok(())
}
```

### Helper: Pure AB payment (no coins)

```rust
fn use_pure_ab_payment(
    transaction: &mut TransactionData,
    current_epoch: EpochId,
) -> Result<()> {
    // 1. Clear gas payment (signals pure AB payment)
    transaction.gas_data_mut().payment.clear();

    // 2. MUST set ValidDuring expiration for AB payment
    set_valid_during_expiration(transaction, current_epoch);

    Ok(())
}
```

### Helper: Set expiration

```rust
fn set_valid_during_expiration(
    transaction: &mut TransactionData,
    current_epoch: EpochId,
) {
    *transaction.expiration_mut() = TransactionExpiration::ValidDuring {
        min_epoch: Some(current_epoch),
        max_epoch: Some(current_epoch.saturating_add(1)),
        min_timestamp: None,
        max_timestamp: None,
        chain: chain_id,
        nonce: generate_nonce(),
    };
}
```

## Test Cases

Located in: `crates/sui-e2e-tests/tests/grpc_simulate_gas_coin_tests.rs`

### Test 1: Has AB + has coins + GasCoin used

- Setup: Sender has coins + 5 SUI in AB, PTB uses GasCoin
- Expected: Coin reservation FIRST, transaction succeeds, user can access combined balance via GasCoin

### Test 2: Has AB + has coins + GasCoin NOT used

- Setup: Sender has coins + 5 SUI in AB, PTB does NOT use GasCoin
- Expected: Use AB if sufficient, else coins (no coin reservation needed)

### Test 3: Has AB + NO coins

- Setup: Sender has SUI in AB only, no coins
- Expected: Pure AB payment (empty gas_data.payment), expiration set

### Test 4: NO AB + has coins

- Setup: Sender has coins only, no AB
- Expected: Traditional coin gas payment

### Test 5: Insufficient total funds

- Setup: Neither coins nor AB can cover the request
- Expected: Fails with insufficient funds

### Test 6: Protocol config disabled

- Setup: Accumulators not enabled
- Expected: Falls back to traditional coin selection

### Test 7: Combined AB + coins needed

- Setup: Sender has coins + AB, but neither alone is sufficient for the requested amount
- Expected: Compat layer combines both sources via coin reservation

## Key Points

1. **Smashing priority**: Always try to smash coins into AB when both are available
2. **Pure AB requires expiration**: When `gas_data.payment` is empty, MUST set `ValidDuring` expiration
3. **Coin reservation position**: Always FIRST in payment list to ensure it's used
4. **GasCoin access**: When user references GasCoin, they get access to the smashed result

## Files to Modify

1. `crates/sui-rpc-api/src/grpc/v2/transaction_execution_service/simulate/mod.rs`
   - Update `select_gas()` with the three-way logic
   - Add `insert_coin_reservation_first()` helper
   - Add `use_pure_ab_payment()` helper
   - Add `set_valid_during_expiration()` helper
