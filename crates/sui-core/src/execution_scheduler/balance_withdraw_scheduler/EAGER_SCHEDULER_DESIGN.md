# Eager Balance Withdrawal Scheduler Design

## Overview

The Eager Balance Withdrawal Scheduler is an optimized implementation of the balance withdrawal scheduling system in Sui that enables sequential scheduling of withdrawal transactions without waiting for checkpoint settlements. This design provides faster feedback to users while maintaining safety guarantees about balance availability.

## Key Concepts

### Minimum Guaranteed Balance

The core innovation of the eager scheduler is the concept of **minimum guaranteed balance**, which is calculated as:

```
minimum_guaranteed_balance = settled_balance - sum(reservations_by_version.values())
```

Where:
- `settled_balance`: The last known balance from a checkpoint settlement
- `reservations_by_version`: A map tracking reservations by the version they were made at
- The sum includes all reservations across all versions (both settled and unsettled)

This allows the scheduler to make immediate decisions about whether a withdrawal can succeed without waiting for the next checkpoint.

### Scheduling Outcomes

The scheduler returns exactly three possible outcomes for each withdrawal request:

1. **SufficientBalance**: The withdrawal can proceed as there is enough minimum guaranteed balance
2. **InsufficientBalance**: The withdrawal cannot proceed due to insufficient balance
3. **AlreadyExecuted**: The consensus commit batch has already been processed

## Architecture

### Core Components

#### 1. AccountState
Tracks the state of individual accounts with active withdrawals:

```rust
struct AccountState {
    settled_balance: u64,              // Last known settled balance
    reservations_by_version: BTreeMap<SequenceNumber, u64>, // Reservations tracked by version
    last_settled_version: SequenceNumber,
    entire_balance_reserved_at_version: Option<SequenceNumber>, // Version where entire balance was reserved
}
```

#### 2. EagerSchedulerState
Maintains the global scheduler state:

```rust
struct EagerSchedulerState {
    account_states: BTreeMap<ObjectID, AccountState>,  // Only tracks accounts with withdrawals
    highest_processed_version: SequenceNumber,
    last_settled_version: SequenceNumber,
    pending_insufficient_balance: BTreeMap<SequenceNumber, Vec<(TxBalanceWithdraw, oneshot::Sender<ScheduleResult>)>>,
}
```

#### 3. Reservation Types
Supports two types of balance reservations:
- **MaxAmountU64(amount)**: Reserve a specific amount
- **EntireBalance**: Reserve the entire available balance

### Processing Flow

#### 1. Withdrawal Scheduling

```
1. Check if batch already executed → Return AlreadyExecuted
2. For each transaction in the batch:
   a. Load account states (if not already tracked)
   b. For each reservation in the transaction:
      - Check if sufficient balance available
      - If any check fails:
        * If accumulator_version == last_settled_version → Return InsufficientBalance
        * Otherwise → Mark as pending for settlement
   c. If all checks pass:
      - Apply all reservations atomically
      - Mark as SufficientBalance
3. Clean up accounts with no active reservations
```

#### 2. Settlement Processing

```
1. Update last settled version
2. For each account with balance changes:
   - Apply the change to calculate new settled balance
   - Clear only reservations at versions <= settled version
   - Clear entire_balance_reserved flag if it was at version <= settled version
3. For tracked accounts not in settlement:
   - Fetch latest balance from storage
   - Apply settlement (clearing appropriate reservations)
4. Process pending insufficient balance transactions for this version
   - Re-evaluate with settled balances
   - Make final SufficientBalance/InsufficientBalance decisions
5. Clean up accounts that no longer need tracking
```

### Version Semantics

Understanding the difference between accumulated and settled versions is crucial:

- **Accumulated Version**: The version that a withdrawal would READ from. This represents the state of the accumulator object at the time the withdrawal needs to check balances.
- **Settled Version**: The version that has been WRITTEN/committed. This represents the latest checkpoint where balance changes have been finalized.

A withdrawal with `accumulator_version = N` means it reads the state at version N. Only if `N < last_settled_version` can we be certain the withdrawal has already been executed, because:
- If `N < last_settled_version`: The state at version N has been finalized and any withdrawals reading it have been processed
- If `N = last_settled_version`: The withdrawal might be reading the just-settled state but hasn't been executed yet
- If `N > last_settled_version`: The withdrawal is reading a future state that hasn't been settled

**Protocol Guarantee**: The Sui protocol and consensus handler guarantee that each accumulator version will only be scheduled once. This means a batch with a specific accumulator version can only be scheduled exactly once, preventing any duplicate scheduling at the protocol level.

**Important Balance Loading Consideration**: When loading account balances during withdrawal scheduling, we must use `last_settled_version` rather than the withdrawal's `accumulator_version`. This is because:
- Accounts may have only been settled at a past version, not at the current accumulator version
- Using accumulator_version could read a just-settled balance before `settle_balances` is called
- This would cause double-counting when the settlement is officially processed
- Therefore, we always bound balance reads by `last_settled_version` to ensure consistency

## Key Design Decisions

### 1. Conservative Balance Tracking

The eager scheduler is intentionally more conservative than the naive scheduler. It may reject transactions that would succeed if we waited for settlement, but it never approves transactions that would fail. This ensures safety while providing fast feedback.

### 2. Memory Efficiency

The scheduler only tracks accounts that have active withdrawals. After settlements, accounts with no pending reservations are removed from memory, keeping the memory footprint minimal.

### 3. Immediate Decision Optimization

When a withdrawal's `accumulator_version` equals `last_settled_version`, we can make an immediate InsufficientBalance decision rather than deferring to pending. This is because:
- The withdrawal is trying to read the exact version that was just settled
- No more deposits can occur at this version
- We have complete information to make a final decision

### 4. Atomic Multi-Account Transactions

When a transaction involves multiple accounts, all reservations must succeed or the entire transaction is marked as insufficient. This maintains atomicity at the transaction level.

### 5. Pending Insufficient Balance Tracking

To maintain transaction ordering and prevent race conditions, the scheduler tracks accounts with pending insufficient balance transactions:
- When a transaction fails due to insufficient balance at a future version, all involved accounts are marked
- Subsequent transactions touching any of these accounts are automatically blocked
- This prevents later transactions from "stealing" balance from earlier ones
- The tracking is cleaned up when the version is settled and final decisions are made

### 6. Sequential Processing Within Batches

Transactions within a consensus commit batch are processed sequentially to maintain consistency and predictability. This ensures that the order of transactions matters for balance calculations.

### 7. Version-Based Reservation Tracking

Reservations are tracked by the version they were made at, not as a cumulative sum. This enables:
- Partial settlement: When settling version N, only clear reservations at versions <= N
- Future reservations persist: Reservations at versions > N remain active after settlement
- Accurate balance calculations: The scheduler knows exactly which reservations are still pending

This is crucial for correctness when multiple versions are in flight simultaneously.

## Comparison with Naive Scheduler

| Aspect | Naive Scheduler | Eager Scheduler |
|--------|----------------|-----------------|
| **Waiting for Settlement** | Waits for each version to be settled | Processes immediately |
| **Balance View** | Sees actual settled balance | Uses minimum guaranteed balance |
| **Transaction Approval** | More permissive | More conservative |
| **Performance** | Slower feedback | Immediate feedback |
| **Memory Usage** | Minimal | Tracks active accounts only |

## Example Scenarios

### Scenario 1: Basic Withdrawal Sequence

```
Initial: Account A has 1000 coins
1. Withdraw 600 → Success (1000 - 600 = 400 remaining)
2. Withdraw 300 → Success (400 - 300 = 100 remaining)
3. Withdraw 200 → Failure (only 100 remaining)
4. Settlement occurs: actual balance = 100
5. Withdraw 100 → Success
```

### Scenario 2: Entire Balance Reservation

```
Initial: Account B has 500 coins
1. Withdraw EntireBalance → Success (reserves all 500)
2. Withdraw 1 coin → Failure (entire balance already reserved)
3. Settlement occurs: balance = 0
4. All future withdrawals fail until deposit
```

### Scenario 3: Multi-Account Transaction

```
Initial: Account C has 100, Account D has 200
Transaction with:
  - Withdraw 80 from C
  - Withdraw 150 from D
Result: Success (both accounts have sufficient balance)

Next transaction with:
  - Withdraw 30 from C
  - Withdraw 100 from D
Result: Failure (C only has 20 remaining)
```

### Scenario 4: Version-Based Reservation Tracking

```
Initial: Account E has 100 coins, last_settled_version = 10
1. Version 11: Withdraw 30 → Success (reserves 30 at v11)
2. Version 12: Withdraw 40 → Success (reserves 40 at v12)
3. Version 13: Withdraw 20 → Success (reserves 20 at v13)
   - Total reservations: 90, available: 10
4. Settlement at version 12 with balance = 60
   - Clears reservations at v11 and v12 (total: 70)
   - Keeps reservation at v13 (20)
   - New available: 60 - 20 = 40
5. Version 14: Withdraw 35 → Success (reserves 35 at v14)
```

### Scenario 5: Pending Insufficient Balance Blocking

```
Initial: Account F has 50 coins, last_settled_version = 10
1. Version 11: TX1 attempts to withdraw 100 → Pending Insufficient
   - Cannot immediately determine insufficient (might get deposits)
   - Added to pending_insufficient_balance
   - Account F marked as having pending insufficient
2. Version 11: TX2 attempts to withdraw 30 → Blocked
   - Even though F has 50 coins and TX2 only needs 30
   - Blocked because TX1 is pending on same account
   - Also added to pending_insufficient_balance
3. Settlement at version 11 with no changes (balance still 50)
   - TX1 → Insufficient Balance (confirmed)
   - TX2 → Insufficient Balance (because TX1 was ahead)
```

This blocking behavior ensures transaction ordering is preserved and prevents race conditions where a later transaction could "steal" balance from an earlier one.

## Performance Characteristics

### Advantages
1. **Immediate Feedback**: No waiting for checkpoint settlements
2. **Parallel Processing**: Different accounts can be processed independently
3. **Predictable Behavior**: Sequential processing ensures deterministic results
4. **Efficient Settlement**: Uses version-indexed data structures for O(1) lookup of affected accounts

### Trade-offs
1. **Conservative Decisions**: May reject valid transactions to ensure safety
2. **Memory Overhead**: Tracks state for active accounts
3. **Complexity**: More complex than naive implementation

### Key Optimizations

1. **Version-Based Account Tracking**: The scheduler maintains `accounts_pending_at_version` which maps each version to the set of accounts with pending operations. This enables O(1) lookup during settlement instead of scanning all tracked accounts.

2. **No Balance Reloading**: Once an account is being tracked (has pending reservations), the scheduler maintains its balance through all operations. Withdrawals update the balance during scheduling, and deposits update it during settlement. This eliminates the need to reload balances from storage.

3. **Efficient Cleanup**: The version-based tracking allows precise cleanup of data structures when versions are settled, minimizing memory usage.

## Metrics and Observability

The scheduler includes comprehensive metrics:
- `schedule_outcome_counter`: Tracks scheduling decisions by status
- `tracked_accounts_gauge`: Number of accounts currently being tracked
- `active_reservations_gauge`: Number of active reservations by type
- `settlements_processed_counter`: Number of settlements processed

## Future Enhancements

1. **Optimistic Scheduling**: Could track pending settlements to be less conservative
2. **Batch Optimization**: Could analyze entire batches for better scheduling decisions
3. **Priority Scheduling**: Could prioritize certain types of transactions
4. **Adaptive Thresholds**: Could adjust conservativeness based on network conditions

## Conclusion

The Eager Balance Withdrawal Scheduler provides a significant improvement in user experience by offering immediate feedback on withdrawal requests while maintaining the safety guarantees required by the Sui blockchain. Its conservative approach ensures that approved transactions will succeed, even if some valid transactions are temporarily rejected until settlement.