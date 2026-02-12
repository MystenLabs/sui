# `advance_epoch` Abort Path Analysis

This document catalogs every path through which `advance_epoch` (in
`sui_system_state_inner.move`) can abort, along with the assumptions on
inputs that must hold to prevent each abort. The goal is to later write
tests that attempt to violate each assumption starting from real mainnet
state.

When `advance_epoch` aborts, the network falls back to **safe mode**: gas
fees are stashed and reward distribution is skipped entirely. This is
disruptive, so the function must never abort under any reachable input.

---

## 1. Direct Asserts in `advance_epoch`

### 1a. `EBpsTooLarge` (line 832-836)

```move
assert!(
    storage_fund_reinvest_rate <= BASIS_POINT_DENOMINATOR
    && reward_slashing_rate <= BASIS_POINT_DENOMINATOR,
    EBpsTooLarge,
);
```

**Assumption**: Both `storage_fund_reinvest_rate` and `reward_slashing_rate`
are <= 10,000.

**Source**: These come from `ProtocolConfig` on the Rust side
(`protocol_config.storage_fund_reinvest_rate()` and
`protocol_config.reward_slashing_rate()`). They are static protocol
parameters, not derived from on-chain state.

**Risk**: Very Low. Would require a corrupt protocol config.

---

### 1b. `EAdvancedToWrongEpoch` (line 907)

```move
self.epoch = self.epoch + 1;
assert!(new_epoch == self.epoch, EAdvancedToWrongEpoch);
```

**Assumption**: `new_epoch == self.epoch + 1` where `self.epoch` is the
system state's current epoch counter.

**Source**: `new_epoch` comes from `epoch_store.epoch() + 1` on the Rust
side. `self.epoch` is the on-chain system state.

**Risk**: Very Low. Would require the Rust-side epoch counter to be out
of sync with the on-chain state. Could happen if the system state object
is somehow corrupted or if safe mode previously incremented the epoch
incorrectly.

---

### 1c. `ESafeModeGasNotProcessed` (lines 969-974)

```move
assert!(
    self.safe_mode_storage_rebates == 0
    && self.safe_mode_storage_rewards.value() == 0
    && self.safe_mode_computation_rewards.value() == 0,
    ESafeModeGasNotProcessed,
);
```

**Assumption**: All safe mode accumulated gas has been drained to zero.

**Source**: Lines 843-852 explicitly drain these fields into the main
reward accumulators before any processing. This assertion is a
double-check.

**Risk**: Extremely Low. The code that zeroes these out is 20 lines
above this assert in the same function.

---

## 2. Balance::split Failures (ENotEnough)

`Balance::split(value)` aborts if `self.value < value`. This is the most
dangerous class of abort because the amounts are derived from complex
arithmetic across multiple on-chain values.

### 2a. Storage fund reward split (line 895)

```move
let mut storage_fund_reward = computation_reward.split(storage_fund_reward_amount as u64);
```

Where:
- `storage_fund_reward_amount = storage_fund_balance * computation_charge / total_stake`
- `computation_reward.value() = computation_charge + stake_subsidy_amount`
- `total_stake = storage_fund_balance + total_validators_stake`

**Assumption**: `storage_fund_reward_amount <= computation_reward.value()`

**Analysis**: Since `storage_fund_balance <= total_stake`,
`storage_fund_reward_amount <= computation_charge`. And
`computation_reward.value() >= computation_charge` (it has the subsidy
added). So this split is mathematically safe **as long as `total_stake > 0`**.

**Risk**: Low. Safe unless `total_stake == 0`, which would cause a
division-by-zero first (see section 3).

---

### 2b. Storage fund reinvestment split (line 901-903)

```move
let storage_fund_reinvestment = storage_fund_reward.split(
    storage_fund_reinvestment_amount,
);
```

Where:
- `storage_fund_reinvestment_amount = storage_fund_reward_amount * storage_fund_reinvest_rate / 10000`
- `storage_fund_reinvest_rate <= 10000` (asserted in 1a)

**Assumption**: `storage_fund_reinvestment_amount <= storage_fund_reward_amount`

**Analysis**: Since `storage_fund_reinvest_rate <= 10000`, the numerator
is at most `storage_fund_reward_amount * 10000`, and dividing by 10000
yields at most `storage_fund_reward_amount`. Safe. However, there is
a subtle issue with the `as u64` cast on line 895 vs the u128 result
of `mul_div!`. If the u128 result exceeds u64::MAX, the cast would
silently truncate. In practice the amounts are SUI balances so this
cannot happen.

**Risk**: Very Low.

---

### 2c. `storage_fund::advance_epoch` - non-refundable fee split (storage_fund.move:52)

```move
let non_refundable_storage_fee = self
    .total_object_storage_rebates
    .split(non_refundable_storage_fee_amount);
```

At this point, `total_object_storage_rebates` has just had
`storage_charges` (from `storage_reward`) joined in.

**Assumption**: `non_refundable_storage_fee_amount <= total_object_storage_rebates + storage_charges`

Where `non_refundable_storage_fee_amount` includes the epoch's
non-refundable fees plus any accumulated safe-mode non-refundable fees.

**Source**: `non_refundable_storage_fee_amount` comes from the
`GasCostSummary` accumulated across all transactions in the epoch (plus
safe mode leftovers). The `total_object_storage_rebates` is the on-chain
storage fund rebate pool.

**Risk**: **MEDIUM**. If the accumulated `non_refundable_storage_fee_amount`
from the epoch exceeds the storage rebate pool (after adding storage
charges), this will abort. This could happen if:
- The storage rebate pool has been depleted over many epochs
- An unusually large amount of non-refundable fees accumulates
- Safe mode leftovers push the amount over the edge

---

### 2d. `storage_fund::advance_epoch` - storage rebate split (storage_fund.move:58)

```move
let storage_rebate = self.total_object_storage_rebates.split(storage_rebate_amount);
```

After the previous split (2c), the remaining balance is
`total_object_storage_rebates + storage_charges - non_refundable_storage_fee_amount`.

**Assumption**: `storage_rebate_amount <= total_object_storage_rebates + storage_charges - non_refundable_storage_fee_amount`

**Source**: `storage_rebate_amount` is the sum of all storage rebates
refunded to transaction senders during the epoch, plus safe mode
leftovers.

**Risk**: **MEDIUM-HIGH**. This is the most dangerous Balance::split in
the function. The invariant that the storage rebate pool always has
enough to cover rebates depends on proper accounting across the entire
epoch. If:
- Many objects with high storage rebates are deleted in one epoch
- The storage fund hasn't grown proportionally
- Safe mode leftovers accumulated over multiple safe-mode epochs
Then this split could fail.

---

### 2e. `distribute_reward` - staking reward split (validator_set.move:1271)

```move
let mut staker_reward = staking_rewards.split(staking_reward_amount);
```

And validator commission split (validator_set.move:1278):

```move
let mut validator_reward = staker_reward.split(validator_commission_amount);
```

And storage fund reward split (validator_set.move:1281):

```move
validator_reward.join(storage_fund_reward.split(adjusted_storage_fund_reward_amounts[i]));
```

**Assumption**: The sum of all `adjusted_staking_reward_amounts[i]` <=
`computation_reward.value()` (after storage fund share removed), and
each `validator_commission_amount <= staking_reward_amount`, and the sum
of all `adjusted_storage_fund_reward_amounts[i]` <=
`storage_fund_reward.value()`.

**Analysis**: The adjusted amounts are computed from
`compute_adjusted_reward_distribution`, which redistributes slashed
validator rewards to non-slashed validators. Due to integer truncation
in `mul_div!`, the sum of adjusted amounts should be <= the original
totals. However, the **redistribution adds** rewards to non-slashed
validators, and these additions use `mul_div!` which truncates. So the
sum should be safe.

The commission split is safe because `validator_commission_amount =
staking_reward_amount * commission_rate / 10000`, and `commission_rate
<= 10000` (enforced at validator creation).

**Risk**: Low. Integer truncation should always produce sums <=
available balance. But worth verifying with extreme reward distributions.

---

## 3. Division by Zero

### 3a. `mul_div!(storage_fund_balance, computation_charge, total_stake)` (line 890-894)

```move
let storage_fund_reward_amount = mul_div!(
    storage_fund_balance,
    computation_charge,
    total_stake,
);
```

`mul_div!` expands to `((a as u128) * (b as u128) / (c as u128)) as u64`.

**Assumption**: `total_stake > 0`

Where `total_stake = storage_fund_balance + total_validators_stake`.

**Risk**: Very Low. Requires both the storage fund and all validator
stakes to be zero. Impossible on mainnet.

---

### 3b. `total_storage_fund_reward / length` in `compute_unadjusted_reward_distribution` (validator_set.move:1174)

**Assumption**: `validators.length() > 0` (at least one active validator).

**Risk**: Very Low. The network cannot function with zero validators.

---

### 3c. `total_staking_reward_adjustment * voting_power / total_unslashed_validator_voting_power` in `compute_adjusted_reward_distribution` (validator_set.move:1231)

**Assumption**: `total_unslashed_validator_voting_power > 0` (not all
validators are slashed).

**Analysis**: For a validator to be slashed, a quorum (2/3+) of voting
power must report them. It is mathematically impossible for every
validator to be reported by a quorum, because a validator cannot report
itself (and 2/3 reporting + the reported validator > 100%).

Actually, the reporting mechanism allows a validator to be in the
reporter set AND be reported. So this needs more careful analysis.

**Risk**: Low but non-trivial. Needs careful analysis of the tallying
rule.

---

### 3d. `total_storage_fund_reward_adjustment / num_unslashed_validators` in `compute_adjusted_reward_distribution` (validator_set.move:1248)

**Assumption**: `num_unslashed_validators > 0` (same as 3c).

**Risk**: Same as 3c.

---

## 4. Validator Set Assertions

### 4a. `ENonValidatorInReportRecords` (validator_set.move:1144-1146)

```move
assert!(
    self.is_active_validator_by_sui_address(validator_address),
    ENonValidatorInReportRecords,
);
```

**Assumption**: Every address in `validator_report_records` is an active
validator.

**Source**: Report records are populated by `report_validator` calls
during the epoch, which check that the reported address is an active
validator at the time of the call. However, validators can be removed
(pending_removals) before the next epoch. If a validator is reported
and then submits a removal request in the same epoch, they would still
be active at this point (removals are processed later in `advance_epoch`).

But what about validators removed in a **previous** epoch? The report
records persist across epochs. Are stale entries cleaned up?

**Risk**: **MEDIUM**. Needs investigation of whether stale report records
are cleaned up when validators depart. If a validator departs and their
address remains in `validator_report_records`, the next `advance_epoch`
would abort here.

---

### 4b. `EValidatorSetEmpty` (validator_set.move:1266)

```move
assert!(length > 0, EValidatorSetEmpty);
```

**Assumption**: The active validator set is non-empty.

**Risk**: Extremely Low.

---

### 4c. `ENotAValidator` in `get_validator_indices` (validator_set.move:807)

```move
let idx = find_validator(validators, *addr).destroy_or!(abort ENotAValidator);
```

**Assumption**: Every slashed validator address maps to an active validator.

**Source**: `slashed_validators` is produced by `compute_slashed_validators`
which only yields addresses that pass `is_active_validator_by_sui_address`.
So this should always hold.

**Risk**: Very Low.

---

## 5. Voting Power Invariants

### 5a. `set_voting_power` assertions (voting_power.move)

After processing all validator changes, `set_voting_power` is called,
which enforces:

- `ETotalPowerMismatch`: Sum of all voting powers must equal
  `TOTAL_VOTING_POWER` (10,000)
- `EVotingPowerOverThreshold`: No individual voting power exceeds the
  threshold
- `EInvalidVotingPower`: Every validator has voting power > 0
- `ERelativePowerMismatch`: Ordering of voting powers matches ordering
  of stakes

**Assumption**: The voting power allocation algorithm always finds a
valid solution for any stake distribution among active validators.

**Risk**: **MEDIUM**. The algorithm is complex and involves iterative
adjustments. Edge cases with extreme stake distributions (e.g., one
validator with 99.99% of stake) or very large numbers of validators
could potentially cause invariant violations. The `ERelativePowerMismatch`
check is particularly sensitive since integer rounding could cause ties
to be ordered incorrectly.

---

## 6. Stake Subsidy

### 6a. `stake_subsidy::advance_epoch` division by zero

```move
if (self.distribution_counter % self.stake_subsidy_period_length == 0) {
```

**Assumption**: `stake_subsidy_period_length > 0`.

**Source**: Set during genesis/creation. On mainnet this is 10.

**Risk**: Extremely Low.

---

### 6b. Safe mode catch-up loop (line 874-883)

```move
if (self.stake_subsidy.get_distribution_counter() == 540 && old_epoch > 560) {
    let first_safe_mode_epoch = 560;
    let safe_mode_epoch_count = old_epoch - first_safe_mode_epoch;
    safe_mode_epoch_count.do!(|_| {
        stake_subsidy.join(self.stake_subsidy.advance_epoch());
    });
};
```

**Assumption**: This runs at most once (guarded by
`distribution_counter == 540`). On mainnet, `distribution_counter` is
already past 540, so this code is dead.

**Risk**: None (dead code on mainnet).

---

## 7. u64 Overflow

### 7a. `self.epoch = self.epoch + 1` (line 905)

**Assumption**: `self.epoch < u64::MAX`.

**Risk**: Impossible to reach in practice.

---

### 7b. Accumulation of safe mode values (lines 848-851)

```move
storage_rebate_amount = storage_rebate_amount + self.safe_mode_storage_rebates;
non_refundable_storage_fee_amount =
    non_refundable_storage_fee_amount + self.safe_mode_non_refundable_storage_fee;
```

**Assumption**: These additions don't overflow u64.

**Risk**: Extremely Low. Would require astronomical SUI amounts.

---

## Summary: Risk-Ordered Abort Paths

| # | Risk | Abort Path | Section |
|---|------|-----------|---------|
| 1 | **MEDIUM-HIGH** | Storage rebate split in `storage_fund::advance_epoch` | 2d |
| 2 | **MEDIUM** | Non-refundable fee split in `storage_fund::advance_epoch` | 2c |
| 3 | **MEDIUM** | `ENonValidatorInReportRecords` - stale report records | 4a |
| 4 | **MEDIUM** | Voting power invariant violations in `set_voting_power` | 5a |
| 5 | Low | Division by zero if all validators slashed | 3c, 3d |
| 6 | Low | Reward distribution Balance::split rounding | 2e |
| 7 | Very Low | `EBpsTooLarge` from protocol config | 1a |
| 8 | Very Low | `EAdvancedToWrongEpoch` epoch mismatch | 1b |
| 9 | Very Low | Division by zero from zero total stake | 3a |
| 10 | Very Low | `EValidatorSetEmpty` | 4b |
| 11 | Extremely Low | `ESafeModeGasNotProcessed` | 1c |
| 12 | Extremely Low | u64 overflow | 7a, 7b |

---

## Proposed Test Plan

The tests should use the existing `TestRunner` infrastructure in
`sui-system/tests/builders/test_runner.move` and focus on the medium and
higher risk abort paths.

### Test Category 1: Storage Fund Balance Splits (Sections 2c, 2d)

**Goal**: Verify that `storage_fund::advance_epoch` never aborts for
realistic combinations of storage charges, rebates, and non-refundable fees.

Tests:
1. **Large storage rebate relative to fund**: Set up a storage fund with
   a small `total_object_storage_rebates`, then call `advance_epoch` with
   a large `storage_rebate_amount` approaching the fund balance.
2. **Large non-refundable fee relative to fund**: Same setup but with a
   large `non_refundable_storage_fee_amount`.
3. **Combined pressure**: Both `storage_rebate_amount` and
   `non_refundable_storage_fee_amount` are large relative to the fund.
4. **After safe mode accumulation**: Simulate multiple safe mode epochs
   that accumulate rebates and fees, then attempt a normal `advance_epoch`.
5. **Zero storage charges with non-zero rebates**: Test the edge case
   where no new storage was charged but rebates are requested.

### Test Category 2: Validator Report Records (Section 4a)

**Goal**: Verify that stale report records for departed validators don't
cause `advance_epoch` to abort.

Tests:
1. **Report then remove validator**: Report a validator, then have them
   submit a removal request, then advance epoch.
2. **Report then validator loses stake**: Report a validator, have their
   stake drop below the low-stake threshold, advance epoch so they get
   kicked, then advance epoch again with stale records.

### Test Category 3: Voting Power Invariants (Section 5a)

**Goal**: Verify `set_voting_power` never fails with extreme stake
distributions.

Tests:
1. **Extreme stake inequality**: One validator with 99.99% of stake,
   many validators with dust stakes.
2. **All equal stakes**: Every validator has exactly the same stake.
3. **Minimum validator count**: Only 1 active validator.
4. **Large validator count**: Many validators near the maximum.
5. **Stake changes at epoch boundary**: Large pending stakes/withdrawals
   that dramatically change the distribution at epoch change.

### Test Category 4: Reward Distribution Rounding (Sections 2e, 3c, 3d)

**Goal**: Verify reward distribution doesn't cause Balance::split
failures due to rounding.

Tests:
1. **Large rewards with many validators**: Verify the sum of adjusted
   rewards never exceeds the available balance.
2. **All validators slashed**: Attempt to slash all validators and verify
   the function handles it (or verify it's impossible).
3. **Single validator slashed with maximum slashing rate**: Test the
   redistribution math at the extreme.
4. **Zero computation reward**: No gas fees in an epoch, so all reward
   pools are zero.
5. **Dust rewards**: Very small rewards (1 MIST) distributed across
   many validators.

### Test Category 5: Edge Cases from Mainnet State

**Goal**: Use realistic mainnet-like state to stress test.

Tests:
1. **Mainnet validator count and stake distribution**: Replicate the
   approximate validator count (~100+) and stake distribution from
   mainnet.
2. **Mainnet storage fund size**: Use realistic storage fund balances.
3. **Mainnet epoch gas totals**: Use realistic gas cost summaries.
4. **Post-safe-mode recovery**: Simulate the state after multiple safe
   mode epochs and verify recovery.
