# Eager Balance Withdrawal Scheduler Design

## Overview

The Eager Balance Withdrawal Scheduler is an optimized implementation of the balance withdrawal scheduling system in Sui that enables sequential scheduling of withdrawal transactions without waiting for checkpoint settlements. This design provides faster feedback to users while maintaining safety guarantees about balance availability.

## Key Concepts

### Minimum Guaranteed Balance

The core innovation of the eager scheduler is the concept of **minimum guaranteed balance**, which is calculated as:

```
minimum_guaranteed_balance = settled_balance - cumulative_reservations
```

Where:
- `settled_balance`: The last known balance from a checkpoint settlement
- `cumulative_reservations`: The sum of all reservations scheduled since the last settlement

This allows the scheduler to make immediate decisions about whether a withdrawal can succeed without waiting for the next checkpoint.

### Scheduling Outcomes

The scheduler returns exactly three possible outcomes for each withdrawal request:

1. **SufficientBalance**: The withdrawal can proceed as there is enough minimum guaranteed balance
2. **InsufficientBalance**: The withdrawal cannot proceed due to insufficient balance
3. **AlreadyScheduled**: The consensus commit batch has already been processed

## Architecture

### Core Components

#### 1. AccountState
Tracks the state of individual accounts with active withdrawals:

```rust
struct AccountState {
    settled_balance: u64,              // Last known settled balance
    cumulative_reservations: u64,      // Sum of all reservations since settlement
    last_settled_version: SequenceNumber,
    entire_balance_reserved: bool,     // Flag for EntireBalance reservations
}
```

#### 2. EagerSchedulerState
Maintains the global scheduler state:

```rust
struct EagerSchedulerState {
    account_states: BTreeMap<ObjectID, AccountState>,  // Only tracks accounts with withdrawals
    scheduled_batches: ScheduledBatches,               // Prevents double scheduling
    highest_processed_version: SequenceNumber,
    last_settled_version: SequenceNumber,
}
```

#### 3. Reservation Types
Supports two types of balance reservations:
- **MaxAmountU64(amount)**: Reserve a specific amount
- **EntireBalance**: Reserve the entire available balance

### Processing Flow

#### 1. Withdrawal Scheduling

```
1. Check if batch already scheduled → Return AlreadyScheduled
2. For each transaction in the batch:
   a. Load account states (if not already tracked)
   b. For each reservation in the transaction:
      - Check if sufficient balance available
      - If any check fails, rollback and mark as InsufficientBalance
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
   - Reset cumulative_reservations to 0
   - Clear entire_balance_reserved flag
3. For tracked accounts not in settlement:
   - Fetch latest balance from storage
   - Apply settlement with no changes
4. Clean up old scheduled batch entries
```

## Key Design Decisions

### 1. Conservative Balance Tracking

The eager scheduler is intentionally more conservative than the naive scheduler. It may reject transactions that would succeed if we waited for settlement, but it never approves transactions that would fail. This ensures safety while providing fast feedback.

### 2. Memory Efficiency

The scheduler only tracks accounts that have active withdrawals. After settlements, accounts with no pending reservations are removed from memory, keeping the memory footprint minimal.

### 3. Atomic Multi-Account Transactions

When a transaction involves multiple accounts, all reservations must succeed or the entire transaction is marked as insufficient. This maintains atomicity at the transaction level.

### 4. Sequential Processing Within Batches

Transactions within a consensus commit batch are processed sequentially to maintain consistency and predictability. This ensures that the order of transactions matters for balance calculations.

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

## Performance Characteristics

### Advantages
1. **Immediate Feedback**: No waiting for checkpoint settlements
2. **Parallel Processing**: Different accounts can be processed independently
3. **Predictable Behavior**: Sequential processing ensures deterministic results

### Trade-offs
1. **Conservative Decisions**: May reject valid transactions to ensure safety
2. **Memory Overhead**: Tracks state for active accounts
3. **Complexity**: More complex than naive implementation

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