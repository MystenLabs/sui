// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(unused_const)]
module sui_system::staking_pool;

use sui::bag::{Self, Bag};
use sui::balance::{Self, Balance};
use sui::sui::SUI;
use sui::table::{Self, Table};

/// StakedSui objects cannot be split to below this amount.
const MIN_STAKING_THRESHOLD: u64 = 1_000_000_000; // 1 SUI

const EInsufficientPoolTokenBalance: u64 = 0;
const EWrongPool: u64 = 1;
const EWithdrawAmountCannotBeZero: u64 = 2;
const EInsufficientSuiTokenBalance: u64 = 3;
const EInsufficientRewardsPoolBalance: u64 = 4;
const EDestroyNonzeroBalance: u64 = 5;
const ETokenTimeLockIsSome: u64 = 6;
const EWrongDelegation: u64 = 7;
const EPendingDelegationDoesNotExist: u64 = 8;
const ETokenBalancesDoNotMatchExchangeRate: u64 = 9;
const EDelegationToInactivePool: u64 = 10;
const EDeactivationOfInactivePool: u64 = 11;
const EIncompatibleStakedSui: u64 = 12;
const EWithdrawalInSameEpoch: u64 = 13;
const EPoolAlreadyActive: u64 = 14;
const EPoolPreactiveOrInactive: u64 = 15;
const EActivationOfInactivePool: u64 = 16;
const EDelegationOfZeroSui: u64 = 17;
const EStakedSuiBelowThreshold: u64 = 18;
const ECannotMintFungibleStakedSuiYet: u64 = 19;
const EInvariantFailure: u64 = 20;

/// A staking pool embedded in each validator struct in the system state object.
public struct StakingPool has key, store {
    id: UID,
    /// The epoch at which this pool became active.
    /// The value is `None` if the pool is pre-active and `Some(<epoch_number>)` if active or inactive.
    activation_epoch: Option<u64>,
    /// The epoch at which this staking pool ceased to be active. `None` = {pre-active, active},
    /// `Some(<epoch_number>)` if in-active, and it was de-activated at epoch `<epoch_number>`.
    deactivation_epoch: Option<u64>,
    /// The total number of SUI tokens in this pool, including the SUI in the rewards_pool, as well as in all the principal
    /// in the `StakedSui` object, updated at epoch boundaries.
    sui_balance: u64,
    /// The epoch stake rewards will be added here at the end of each epoch.
    rewards_pool: Balance<SUI>,
    /// Total number of pool tokens issued by the pool.
    pool_token_balance: u64,
    /// Exchange rate history of previous epochs. Key is the epoch number.
    /// The entries start from the `activation_epoch` of this pool and contains exchange rates at the beginning of each epoch,
    /// i.e., right after the rewards for the previous epoch have been deposited into the pool.
    exchange_rates: Table<u64, PoolTokenExchangeRate>,
    /// Pending stake amount for this epoch, emptied at epoch boundaries.
    pending_stake: u64,
    /// Pending stake withdrawn during the current epoch, emptied at epoch boundaries.
    /// This includes both the principal and rewards SUI withdrawn.
    pending_total_sui_withdraw: u64,
    /// Pending pool token withdrawn during the current epoch, emptied at epoch boundaries.
    pending_pool_token_withdraw: u64,
    /// Any extra fields that's not defined statically.
    extra_fields: Bag,
}

/// Struct representing the exchange rate of the stake pool token to SUI.
public struct PoolTokenExchangeRate has copy, drop, store {
    sui_amount: u64,
    pool_token_amount: u64,
}

/// A self-custodial object holding the staked SUI tokens.
public struct StakedSui has key, store {
    id: UID,
    /// ID of the staking pool we are staking with.
    pool_id: ID,
    /// The epoch at which the stake becomes active.
    stake_activation_epoch: u64,
    /// The staked SUI tokens.
    principal: Balance<SUI>,
}

/// An alternative to `StakedSui` that holds the pool token amount instead of the SUI balance.
/// StakedSui objects can be converted to FungibleStakedSuis after the initial warmup period.
/// The advantage of this is that you can now merge multiple StakedSui objects from different
/// activation epochs into a single FungibleStakedSui object.
public struct FungibleStakedSui has key, store {
    id: UID,
    /// ID of the staking pool we are staking with.
    pool_id: ID,
    /// The pool token amount.
    value: u64,
}

/// Holds useful information
public struct FungibleStakedSuiData has key, store {
    id: UID,
    /// fungible_staked_sui supply
    total_supply: u64,
    /// principal balance. Rewards are withdrawn from the reward pool
    principal: Balance<SUI>,
}

// === dynamic field keys ===
public struct FungibleStakedSuiDataKey has copy, drop, store {}

/// Holds the amount of SUI that was underflowed when withdrawing from the pool
/// post safe mode. Cleaned up in the same transaction.
public struct UnderflowSuiBalance has copy, drop, store {}

// ==== initializer ====

/// Create a new, empty staking pool.
public(package) fun new(ctx: &mut TxContext): StakingPool {
    StakingPool {
        id: object::new(ctx),
        activation_epoch: option::none(),
        deactivation_epoch: option::none(),
        sui_balance: 0,
        rewards_pool: balance::zero(),
        pool_token_balance: 0,
        exchange_rates: table::new(ctx),
        pending_stake: 0,
        pending_total_sui_withdraw: 0,
        pending_pool_token_withdraw: 0,
        extra_fields: bag::new(ctx),
    }
}

// ==== stake requests ====

/// Request to stake to a staking pool. The stake starts counting at the beginning of the next epoch,
public(package) fun request_add_stake(
    pool: &mut StakingPool,
    stake: Balance<SUI>,
    stake_activation_epoch: u64,
    ctx: &mut TxContext,
): StakedSui {
    let sui_amount = stake.value();
    assert!(!pool.is_inactive(), EDelegationToInactivePool);
    assert!(sui_amount > 0, EDelegationOfZeroSui);

    pool.pending_stake = pool.pending_stake + sui_amount;
    StakedSui {
        id: object::new(ctx),
        pool_id: object::id(pool),
        stake_activation_epoch,
        principal: stake,
    }
}

/// Request to withdraw the given stake plus rewards from a staking pool.
/// Both the principal and corresponding rewards in SUI are withdrawn.
/// A proportional amount of pool token withdraw is recorded and processed at epoch change time.
public(package) fun request_withdraw_stake(
    pool: &mut StakingPool,
    staked_sui: StakedSui,
    ctx: &TxContext,
): Balance<SUI> {
    // stake is inactive and the pool is not preactive - allow direct withdraw
    // the reason why we exclude preactive pools is to avoid potential underflow
    // on subtraction, and we need to enforce `pending_stake_withdraw` call.
    if (staked_sui.stake_activation_epoch > ctx.epoch() && !pool.is_preactive()) {
        let principal = staked_sui.into_balance();
        pool.pending_stake = pool.pending_stake - principal.value();
        return principal
    };

    let (pool_token_withdraw_amount, mut principal_withdraw) = pool.withdraw_from_principal(
        staked_sui,
    );
    let principal_withdraw_amount = principal_withdraw.value();

    let rewards_withdraw = pool.withdraw_rewards(
        principal_withdraw_amount,
        pool_token_withdraw_amount,
        ctx.epoch(),
    );
    let total_sui_withdraw_amount = principal_withdraw_amount + rewards_withdraw.value();

    pool.pending_total_sui_withdraw = pool.pending_total_sui_withdraw + total_sui_withdraw_amount;
    pool.pending_pool_token_withdraw =
        pool.pending_pool_token_withdraw + pool_token_withdraw_amount;

    // If the pool is inactive or preactive, we immediately process the withdrawal.
    if (pool.is_inactive() || pool.is_preactive()) pool.process_pending_stake_withdraw();

    // TODO: implement withdraw bonding period here.
    principal_withdraw.join(rewards_withdraw);
    principal_withdraw
}

public(package) fun redeem_fungible_staked_sui(
    pool: &mut StakingPool,
    fungible_staked_sui: FungibleStakedSui,
    ctx: &TxContext,
): Balance<SUI> {
    let FungibleStakedSui { id, pool_id, value } = fungible_staked_sui;
    assert!(pool_id == object::id(pool), EWrongPool);

    id.delete();

    let latest_exchange_rate = pool.pool_token_exchange_rate_at_epoch(ctx.epoch());
    let fungible_staked_sui_data: &mut FungibleStakedSuiData =
        &mut pool.extra_fields[FungibleStakedSuiDataKey {}];

    let (
        principal_amount,
        rewards_amount,
    ) = latest_exchange_rate.calculate_fungible_staked_sui_withdraw_amount(
        value,
        fungible_staked_sui_data.principal.value(),
        fungible_staked_sui_data.total_supply,
    );

    fungible_staked_sui_data.total_supply = fungible_staked_sui_data.total_supply - value;

    let mut sui_out = fungible_staked_sui_data.principal.split(principal_amount);
    sui_out.join(pool.rewards_pool.split(rewards_amount));

    pool.pending_total_sui_withdraw = pool.pending_total_sui_withdraw + sui_out.value();
    pool.pending_pool_token_withdraw = pool.pending_pool_token_withdraw + value;

    sui_out
}

/// written in separate function so i can test with random values
/// returns (principal_withdraw_amount, rewards_withdraw_amount)
fun calculate_fungible_staked_sui_withdraw_amount(
    latest_exchange_rate: PoolTokenExchangeRate,
    fungible_staked_sui_value: u64,
    fungible_staked_sui_data_principal_amount: u64, // fungible_staked_sui_data.principal.value()
    fungible_staked_sui_data_total_supply: u64, // fungible_staked_sui_data.total_supply
): (u64, u64) {
    // 1. if the entire FungibleStakedSuiData supply is redeemed, how much sui should we receive?
    let total_sui_amount = latest_exchange_rate.get_sui_amount(
        fungible_staked_sui_data_total_supply,
    );

    // min with total_sui_amount to prevent underflow
    let fungible_staked_sui_data_principal_amount = fungible_staked_sui_data_principal_amount.min(
        total_sui_amount,
    );

    // 2. how much do we need to withdraw from the rewards pool?
    let total_rewards = total_sui_amount - fungible_staked_sui_data_principal_amount;

    // 3. proportionally withdraw from both wrt the fungible_staked_sui_value.
    let principal_withdraw_amount = mul_div!(
        fungible_staked_sui_value,
        fungible_staked_sui_data_principal_amount,
        fungible_staked_sui_data_total_supply,
    );

    let rewards_withdraw_amount = mul_div!(
        fungible_staked_sui_value,
        total_rewards,
        fungible_staked_sui_data_total_supply,
    );

    // invariant check, just in case
    let expected_sui_amount = latest_exchange_rate.get_sui_amount(fungible_staked_sui_value);
    assert!(
        principal_withdraw_amount + rewards_withdraw_amount <= expected_sui_amount,
        EInvariantFailure,
    );

    (principal_withdraw_amount, rewards_withdraw_amount)
}

/// Convert the given staked SUI to an FungibleStakedSui object
public(package) fun convert_to_fungible_staked_sui(
    pool: &mut StakingPool,
    staked_sui: StakedSui,
    ctx: &mut TxContext,
): FungibleStakedSui {
    let StakedSui { id, pool_id, stake_activation_epoch, principal } = staked_sui;

    assert!(pool_id == object::id(pool), EWrongPool);
    assert!(ctx.epoch() >= stake_activation_epoch, ECannotMintFungibleStakedSuiYet);
    assert!(!pool.is_preactive() && !pool.is_inactive(), EPoolPreactiveOrInactive);

    id.delete();

    let exchange_rate_at_staking_epoch = pool.pool_token_exchange_rate_at_epoch(
        stake_activation_epoch,
    );

    let pool_token_amount = exchange_rate_at_staking_epoch.get_token_amount(principal.value());
    let key = FungibleStakedSuiDataKey {};

    if (!pool.extra_fields.contains(key)) {
        pool
            .extra_fields
            .add(
                key,
                FungibleStakedSuiData {
                    id: object::new(ctx),
                    total_supply: pool_token_amount,
                    principal,
                },
            );
    } else {
        let fungible_staked_sui_data: &mut FungibleStakedSuiData = &mut pool.extra_fields[key];
        fungible_staked_sui_data.total_supply =
            fungible_staked_sui_data.total_supply + pool_token_amount;
        fungible_staked_sui_data.principal.join(principal);
    };

    FungibleStakedSui {
        id: object::new(ctx),
        pool_id,
        value: pool_token_amount,
    }
}

/// Withdraw the principal SUI stored in the StakedSui object, and calculate the corresponding amount of pool
/// tokens using exchange rate at staking epoch.
/// Returns values are amount of pool tokens withdrawn and withdrawn principal portion of SUI.
public(package) fun withdraw_from_principal(
    pool: &StakingPool,
    staked_sui: StakedSui,
): (u64, Balance<SUI>) {
    // Check that the stake information matches the pool.
    assert!(staked_sui.pool_id == object::id(pool), EWrongPool);

    let exchange_rate_at_staking_epoch = pool.pool_token_exchange_rate_at_epoch(staked_sui.stake_activation_epoch);
    let principal_withdraw = staked_sui.into_balance();
    let pool_token_withdraw_amount = exchange_rate_at_staking_epoch.get_token_amount(principal_withdraw.value());

    (pool_token_withdraw_amount, principal_withdraw)
}

/// Allows calling `.into_balance()` on `StakedSui` to invoke `unwrap_staked_sui`
use fun unwrap_staked_sui as StakedSui.into_balance;

fun unwrap_staked_sui(staked_sui: StakedSui): Balance<SUI> {
    let StakedSui { id, principal, .. } = staked_sui;
    id.delete();
    principal
}

// ==== functions called at epoch boundaries ===

/// Called at epoch advancement times to add rewards (in SUI) to the staking pool.
public(package) fun deposit_rewards(pool: &mut StakingPool, rewards: Balance<SUI>) {
    pool.sui_balance = pool.sui_balance + rewards.value();
    pool.rewards_pool.join(rewards);
}

public(package) fun process_pending_stakes_and_withdraws(pool: &mut StakingPool, ctx: &TxContext) {
    let new_epoch = ctx.epoch() + 1;
    pool.process_pending_stake_withdraw();
    pool.process_pending_stake();
    pool
        .exchange_rates
        .add(
            new_epoch,
            PoolTokenExchangeRate {
                sui_amount: pool.sui_balance,
                pool_token_amount: pool.pool_token_balance,
            },
        );

    pool.check_balance_invariants(new_epoch);
}

/// Called at epoch boundaries to process pending stake withdraws requested during the epoch.
/// Also called immediately upon withdrawal if the pool is inactive.
fun process_pending_stake_withdraw(pool: &mut StakingPool) {
    pool.sui_balance = if (pool.sui_balance >= pool.pending_total_sui_withdraw) {
        pool.sui_balance - pool.pending_total_sui_withdraw
    } else {
        // the diff will be applied in the `process_pending_stake` function.
        let diff = pool.pending_total_sui_withdraw - pool.sui_balance;
        pool.extra_fields.add(UnderflowSuiBalance {}, diff);
        0
    };

    pool.pool_token_balance = if (pool.pool_token_balance >= pool.pending_pool_token_withdraw) {
        pool.pool_token_balance - pool.pending_pool_token_withdraw
    } else {
        0
    };

    pool.pending_total_sui_withdraw = 0;
    pool.pending_pool_token_withdraw = 0;
}

/// Called at epoch boundaries to process the pending stake.
public(package) fun process_pending_stake(pool: &mut StakingPool) {
    // Use the most up to date exchange rate with the rewards deposited and withdraws effectuated.
    let latest_exchange_rate = PoolTokenExchangeRate {
        sui_amount: pool.sui_balance,
        pool_token_amount: pool.pool_token_balance,
    };

    // This key is only present if the `sui_balance` underflowed, hence, the current value of `sui_balance`
    // is `0`. Pool token balance will be recalculated automatically for `0` value.
    let sui_diff = {
        let key = UnderflowSuiBalance {};
        if (pool.extra_fields.contains(key)) pool.extra_fields.remove(key) else 0
    };

    pool.sui_balance = pool.sui_balance + pool.pending_stake - sui_diff;
    pool.pool_token_balance = latest_exchange_rate.get_token_amount(pool.sui_balance);
    pool.pending_stake = 0;
}

/// This function does the following:
///     1. Calculates the total amount of SUI (including principal and rewards) that the provided pool tokens represent
///        at the current exchange rate.
///     2. Using the above number and the given `principal_withdraw_amount`, calculates the rewards portion of the
///        stake we should withdraw.
///     3. Withdraws the rewards portion from the rewards pool at the current exchange rate. We only withdraw the rewards
///        portion because the principal portion was already taken out of the staker's self custodied StakedSui.
fun withdraw_rewards(
    pool: &mut StakingPool,
    principal_withdraw_amount: u64,
    pool_token_withdraw_amount: u64,
    epoch: u64,
): Balance<SUI> {
    let exchange_rate = pool.pool_token_exchange_rate_at_epoch(epoch);
    let total_sui_withdraw_amount = exchange_rate.get_sui_amount(pool_token_withdraw_amount);
    let mut reward_withdraw_amount = if (total_sui_withdraw_amount >= principal_withdraw_amount) {
        total_sui_withdraw_amount - principal_withdraw_amount
    } else 0;

    // This may happen when we are withdrawing everything from the pool and
    // the rewards pool balance may be less than reward_withdraw_amount.
    // TODO: FIGURE OUT EXACTLY WHY THIS CAN HAPPEN.
    reward_withdraw_amount = reward_withdraw_amount.min(pool.rewards_pool.value());
    pool.rewards_pool.split(reward_withdraw_amount)
}

// ==== preactive pool related ====

/// Called by `validator` module to activate a staking pool.
public(package) fun activate_staking_pool(pool: &mut StakingPool, activation_epoch: u64) {
    // Add the initial exchange rate to the table.
    pool.exchange_rates.add(activation_epoch, initial_exchange_rate());
    // Check that the pool is preactive and not inactive.
    assert!(pool.is_preactive(), EPoolAlreadyActive);
    assert!(!pool.is_inactive(), EActivationOfInactivePool);
    // Fill in the active epoch.
    pool.activation_epoch.fill(activation_epoch);
}

// ==== inactive pool related ====

/// Deactivate a staking pool by setting the `deactivation_epoch`. After
/// this pool deactivation, the pool stops earning rewards. Only stake
/// withdraws can be made to the pool.
public(package) fun deactivate_staking_pool(pool: &mut StakingPool, deactivation_epoch: u64) {
    // We can't deactivate an already deactivated pool.
    assert!(!pool.is_inactive(), EDeactivationOfInactivePool);
    pool.deactivation_epoch = option::some(deactivation_epoch);
}

// ==== getters and misc utility functions ====

public fun sui_balance(pool: &StakingPool): u64 { pool.sui_balance }

public fun pool_id(staked_sui: &StakedSui): ID { staked_sui.pool_id }

public use fun fungible_staked_sui_pool_id as FungibleStakedSui.pool_id;

public fun fungible_staked_sui_pool_id(fungible_staked_sui: &FungibleStakedSui): ID {
    fungible_staked_sui.pool_id
}

/// Allows calling `.amount()` on `StakedSui` to invoke `staked_sui_amount`
public use fun staked_sui_amount as StakedSui.amount;

/// Returns the principal amount of `StakedSui`.
public fun staked_sui_amount(staked_sui: &StakedSui): u64 { staked_sui.principal.value() }

public use fun stake_activation_epoch as StakedSui.activation_epoch;

/// Returns the activation epoch of `StakedSui`.
public fun stake_activation_epoch(staked_sui: &StakedSui): u64 {
    staked_sui.stake_activation_epoch
}

/// Returns true if the input staking pool is preactive.
public fun is_preactive(pool: &StakingPool): bool {
    pool.activation_epoch.is_none()
}

/// Returns the activation epoch of the `StakingPool`. For validator candidates,
/// or pending validators, the value returned is `None`. For active validators,
/// the value is the epoch before the validator was activated.
public(package) fun activation_epoch(pool: &StakingPool): Option<u64> {
    pool.activation_epoch
}

/// Returns true if the input staking pool is inactive.
public fun is_inactive(pool: &StakingPool): bool {
    pool.deactivation_epoch.is_some()
}

public use fun fungible_staked_sui_value as FungibleStakedSui.value;

public fun fungible_staked_sui_value(fungible_staked_sui: &FungibleStakedSui): u64 {
    fungible_staked_sui.value
}

public use fun split_fungible_staked_sui as FungibleStakedSui.split;

public fun split_fungible_staked_sui(
    fungible_staked_sui: &mut FungibleStakedSui,
    split_amount: u64,
    ctx: &mut TxContext,
): FungibleStakedSui {
    assert!(split_amount <= fungible_staked_sui.value, EInsufficientPoolTokenBalance);

    fungible_staked_sui.value = fungible_staked_sui.value - split_amount;

    FungibleStakedSui {
        id: object::new(ctx),
        pool_id: fungible_staked_sui.pool_id,
        value: split_amount,
    }
}

public use fun join_fungible_staked_sui as FungibleStakedSui.join;

public fun join_fungible_staked_sui(self: &mut FungibleStakedSui, other: FungibleStakedSui) {
    let FungibleStakedSui { id, pool_id, value } = other;
    assert!(self.pool_id == pool_id, EWrongPool);

    id.delete();

    self.value = self.value + value;
}

/// Split StakedSui `self` to two parts, one with principal `split_amount`,
/// and the remaining principal is left in `self`.
/// All the other parameters of the StakedSui like `stake_activation_epoch` or `pool_id` remain the same.
public fun split(self: &mut StakedSui, split_amount: u64, ctx: &mut TxContext): StakedSui {
    let original_amount = self.principal.value();
    assert!(split_amount <= original_amount, EInsufficientSuiTokenBalance);
    let remaining_amount = original_amount - split_amount;
    // Both resulting parts should have at least MIN_STAKING_THRESHOLD.
    assert!(remaining_amount >= MIN_STAKING_THRESHOLD, EStakedSuiBelowThreshold);
    assert!(split_amount >= MIN_STAKING_THRESHOLD, EStakedSuiBelowThreshold);
    StakedSui {
        id: object::new(ctx),
        pool_id: self.pool_id,
        stake_activation_epoch: self.stake_activation_epoch,
        principal: self.principal.split(split_amount),
    }
}

/// Allows calling `.split_to_sender()` on `StakedSui` to invoke `split_staked_sui`
public use fun split_staked_sui as StakedSui.split_to_sender;

#[allow(lint(public_entry))]
/// Split the given StakedSui to the two parts, one with principal `split_amount`,
/// transfer the newly split part to the sender address.
public entry fun split_staked_sui(stake: &mut StakedSui, split_amount: u64, ctx: &mut TxContext) {
    transfer::transfer(stake.split(split_amount, ctx), ctx.sender());
}

/// Allows calling `.join()` on `StakedSui` to invoke `join_staked_sui`
public use fun join_staked_sui as StakedSui.join;

#[allow(lint(public_entry))]
/// Consume the staked sui `other` and add its value to `self`.
/// Aborts if some of the staking parameters are incompatible (pool id, stake activation epoch, etc.)
public entry fun join_staked_sui(self: &mut StakedSui, other: StakedSui) {
    assert!(is_equal_staking_metadata(self, &other), EIncompatibleStakedSui);
    let StakedSui { id, principal, .. } = other;

    id.delete();
    self.principal.join(principal);
}

/// Returns true if all the staking parameters of the staked sui except the principal are identical
public fun is_equal_staking_metadata(self: &StakedSui, other: &StakedSui): bool {
    (self.pool_id == other.pool_id) &&
    (self.stake_activation_epoch == other.stake_activation_epoch)
}

public fun pool_token_exchange_rate_at_epoch(
    pool: &StakingPool,
    epoch: u64,
): PoolTokenExchangeRate {
    // If the pool is preactive then the exchange rate is always 1:1.
    if (pool.is_preactive_at_epoch(epoch)) {
        return initial_exchange_rate()
    };
    let clamped_epoch = pool.deactivation_epoch.get_with_default(epoch);
    let mut epoch = clamped_epoch.min(epoch);
    let activation_epoch = *pool.activation_epoch.borrow();

    // Find the latest epoch that's earlier than the given epoch with an entry in the table
    while (epoch >= activation_epoch) {
        if (pool.exchange_rates.contains(epoch)) {
            return pool.exchange_rates[epoch]
        };
        epoch = epoch - 1;
    };
    // This line really should be unreachable. Do we want an assert false here?
    initial_exchange_rate()
}

/// Returns the total value of the pending staking requests for this staking pool.
public fun pending_stake_amount(staking_pool: &StakingPool): u64 {
    staking_pool.pending_stake
}

/// Returns the total withdrawal from the staking pool this epoch.
public fun pending_stake_withdraw_amount(staking_pool: &StakingPool): u64 {
    staking_pool.pending_total_sui_withdraw
}

public(package) fun exchange_rates(pool: &StakingPool): &Table<u64, PoolTokenExchangeRate> {
    &pool.exchange_rates
}

public fun sui_amount(exchange_rate: &PoolTokenExchangeRate): u64 {
    exchange_rate.sui_amount
}

public fun pool_token_amount(exchange_rate: &PoolTokenExchangeRate): u64 {
    exchange_rate.pool_token_amount
}

/// Returns true if the provided staking pool is preactive at the provided epoch.
fun is_preactive_at_epoch(pool: &StakingPool, epoch: u64): bool {
    // Either the pool is currently preactive or the pool's starting epoch is later than the provided epoch.
    pool.is_preactive() || (*pool.activation_epoch.borrow() > epoch)
}

fun get_sui_amount(exchange_rate: &PoolTokenExchangeRate, token_amount: u64): u64 {
    // When either amount is 0, that means we have no stakes with this pool.
    // The other amount might be non-zero when there's dust left in the pool.
    if (exchange_rate.sui_amount == 0 || exchange_rate.pool_token_amount == 0) {
        return token_amount
    };

    mul_div!(exchange_rate.sui_amount, token_amount, exchange_rate.pool_token_amount)
}

fun get_token_amount(exchange_rate: &PoolTokenExchangeRate, sui_amount: u64): u64 {
    // When either amount is 0, that means we have no stakes with this pool.
    // The other amount might be non-zero when there's dust left in the pool.
    if (exchange_rate.sui_amount == 0 || exchange_rate.pool_token_amount == 0) {
        return sui_amount
    };

    mul_div!(exchange_rate.pool_token_amount, sui_amount, exchange_rate.sui_amount)
}

fun initial_exchange_rate(): PoolTokenExchangeRate {
    PoolTokenExchangeRate { sui_amount: 0, pool_token_amount: 0 }
}

fun check_balance_invariants(pool: &StakingPool, epoch: u64) {
    let exchange_rate = pool.pool_token_exchange_rate_at_epoch(epoch);
    // check that the pool token balance and sui balance ratio matches the exchange rate stored.
    let expected = exchange_rate.get_token_amount(pool.sui_balance);
    let actual = pool.pool_token_balance;
    assert!(expected == actual, ETokenBalancesDoNotMatchExchangeRate)
}

macro fun mul_div($a: u64, $b: u64, $c: u64): u64 {
    (($a as u128) * ($b as u128) / ($c as u128)) as u64
}

// Given the `staked_sui` receipt calculate the current rewards (in terms of SUI) for it.
public(package) fun calculate_rewards(
    pool: &StakingPool,
    staked_sui: &StakedSui,
    current_epoch: u64,
): u64 {
    let staked_amount = staked_sui.amount();
    let pool_token_withdraw_amount = {
        let exchange_rate_at_staking_epoch = pool.pool_token_exchange_rate_at_epoch(staked_sui.stake_activation_epoch);
        exchange_rate_at_staking_epoch.get_token_amount(staked_amount)
    };

    let new_epoch_exchange_rate = pool.pool_token_exchange_rate_at_epoch(current_epoch);
    let total_sui_withdraw_amount = new_epoch_exchange_rate.get_sui_amount(
        pool_token_withdraw_amount,
    );

    let mut reward_withdraw_amount = if (total_sui_withdraw_amount >= staked_amount) {
        total_sui_withdraw_amount - staked_amount
    } else 0;
    reward_withdraw_amount = reward_withdraw_amount.min(pool.rewards_pool.value());

    reward_withdraw_amount
}

// ==== test-related functions ====

#[test_only]
public(package) fun fungible_staked_sui_data(pool: &StakingPool): &FungibleStakedSuiData {
    bag::borrow(&pool.extra_fields, FungibleStakedSuiDataKey {})
}

#[test_only]
public use fun fungible_staked_sui_data_total_supply as FungibleStakedSuiData.total_supply;

#[test_only]
public(package) fun fungible_staked_sui_data_total_supply(
    fungible_staked_sui_data: &FungibleStakedSuiData,
): u64 {
    fungible_staked_sui_data.total_supply
}

#[test_only]
public use fun fungible_staked_sui_data_principal_value as FungibleStakedSuiData.principal_value;

#[test_only]
public(package) fun fungible_staked_sui_data_principal_value(
    fungible_staked_sui_data: &FungibleStakedSuiData,
): u64 {
    fungible_staked_sui_data.principal.value()
}

#[test_only]
public(package) fun pending_pool_token_withdraw_amount(pool: &StakingPool): u64 {
    pool.pending_pool_token_withdraw
}

#[test_only]
public(package) fun create_fungible_staked_sui_for_testing(
    self: &StakingPool,
    value: u64,
    ctx: &mut TxContext,
): FungibleStakedSui {
    FungibleStakedSui {
        id: object::new(ctx),
        pool_id: object::id(self),
        value,
    }
}

#[test_only]
public(package) fun process_pending_stake_withdraw_for_testing(pool: &mut StakingPool) {
    pool.process_pending_stake_withdraw()
}

#[test_only]
public(package) fun increase_pending_pool_token_withdraw_for_testing(
    pool: &mut StakingPool,
    delta: u64,
) {
    pool.pending_pool_token_withdraw = pool.pending_pool_token_withdraw + delta
}

#[test_only]
public(package) fun increase_pending_total_sui_withdraw_for_testing(
    pool: &mut StakingPool,
    delta: u64,
) {
    pool.pending_total_sui_withdraw = pool.pending_total_sui_withdraw + delta
}

#[test_only]
public(package) fun pool_token_balance(pool: &StakingPool): u64 {
    pool.pool_token_balance
}

// ==== tests ====

#[random_test]
fun test_calculate_fungible_staked_sui_withdraw_amount(
    mut total_sui_amount: u64,
    // these are all in basis points
    mut pool_token_frac: u16,
    mut fungible_staked_sui_data_total_supply_frac: u16,
    mut fungible_staked_sui_data_principal_frac: u16,
    mut fungible_staked_sui_value_bps: u16,
) {
    total_sui_amount = total_sui_amount.max(1);

    pool_token_frac = pool_token_frac % 10_000;
    fungible_staked_sui_data_total_supply_frac =
        fungible_staked_sui_data_total_supply_frac % 10_000;
    fungible_staked_sui_data_principal_frac = fungible_staked_sui_data_principal_frac % 10_000;
    fungible_staked_sui_value_bps = fungible_staked_sui_value_bps % 10_000;

    let total_pool_token_amount = mul_div!(total_sui_amount, pool_token_frac as u64, 10_000).max(1);

    let exchange_rate = PoolTokenExchangeRate {
        sui_amount: total_sui_amount,
        pool_token_amount: total_pool_token_amount,
    };

    let fungible_staked_sui_data_total_supply = mul_div!(
        total_pool_token_amount,
        fungible_staked_sui_data_total_supply_frac as u64,
        10_000,
    ).max(1);
    let fungible_staked_sui_value = mul_div!(
        fungible_staked_sui_data_total_supply,
        fungible_staked_sui_value_bps as u64,
        10_000,
    );

    let max_principal = exchange_rate.get_sui_amount(fungible_staked_sui_data_total_supply);
    let fungible_staked_sui_data_principal_amount = mul_div!(
        max_principal,
        fungible_staked_sui_data_principal_frac as u64,
        10_000,
    ).max(1);

    let (principal_amount, rewards_amount) = calculate_fungible_staked_sui_withdraw_amount(
        exchange_rate,
        fungible_staked_sui_value,
        fungible_staked_sui_data_principal_amount,
        fungible_staked_sui_data_total_supply,
    );

    let expected_out = exchange_rate.get_sui_amount(fungible_staked_sui_value);

    assert!(principal_amount + rewards_amount <= expected_out, 0);

    let min_out = if (expected_out > 2) expected_out - 2 else 0;
    assert!(principal_amount + rewards_amount >= min_out, 0);
}
