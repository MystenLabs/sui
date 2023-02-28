// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::staking_pool {
    use sui::balance::{Self, Balance};
    use sui::sui::SUI;
    use std::option::{Self, Option};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer;
    use sui::epoch_time_lock::{EpochTimeLock};
    use sui::object::{Self, ID, UID};
    use sui::locked_coin;
    use sui::coin;
    use sui::table_vec::{Self, TableVec};
    use sui::math;
    use sui::table::{Self, Table};

    friend sui::validator;
    friend sui::validator_set;

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

    /// A staking pool embedded in each validator struct in the system state object.
    struct StakingPool has key, store {
        id: UID,
        /// The epoch at which this pool started operating. Should be the epoch at which the validator became active.
        starting_epoch: u64,
        /// The total number of SUI tokens in this pool, including the SUI in the rewards_pool, as well as in all the principal
        /// in the `StakedSui` object, updated at epoch boundaries.
        sui_balance: u64,
        /// The epoch delegation rewards will be added here at the end of each epoch.
        rewards_pool: Balance<SUI>,
        /// Total number of pool tokens issued by the pool.
        pool_token_balance: u64,
        /// Exchange rate history of previous epochs. Key is the epoch number.
        /// The entries start from the `starting_epoch` of this pool and contain exchange rates at the beginning of each epoch,
        /// i.e., right after the rewards for the previous epoch have been deposited into the pool.
        exchange_rates: Table<u64, PoolTokenExchangeRate>,
        /// Pending delegation amount for this epoch.
        pending_delegation: u64,
        /// Delegation withdraws requested during the current epoch. Similar to new delegation, the withdraws are processed
        /// at epoch boundaries. Rewards are withdrawn and distributed after the rewards for the current epoch have come in.
        pending_withdraws: TableVec<PendingWithdrawEntry>,
    }

    /// Struct representing the exchange rate of the delegation pool token to SUI.
    struct PoolTokenExchangeRate has store, copy, drop {
        sui_amount: u64,
        pool_token_amount: u64,
    }

    /// An inactive staking pool associated with an inactive validator.
    /// Only withdraws can be made from this pool.
    struct InactiveStakingPool has key {
        id: UID, // TODO: inherit an ID from active staking pool?
        pool: StakingPool,
    }

    /// Struct representing a pending delegation withdraw.
    struct PendingWithdrawEntry has store {
        delegator: address,
        principal_withdraw_amount: u64,
        pool_token_withdraw_amount: u64,
    }

    /// A self-custodial object holding the staked SUI tokens.
    struct StakedSui has key {
        id: UID,
        /// ID of the staking pool we are staking with.
        pool_id: ID,
        // TODO: keeping this field here because the apps depend on it. consider removing it.
        validator_address: address,
        /// The epoch at which the delegation becomes active.
        delegation_activation_epoch: u64,
        /// The staked SUI tokens.
        principal: Balance<SUI>,
        /// If the stake comes from a Coin<SUI>, this field is None. If it comes from a LockedCoin<SUI>, this
        /// field will record the original lock expiration epoch, to be used when unstaking.
        sui_token_lock: Option<EpochTimeLock>,
    }

    // ==== initializer ====

    /// Create a new, empty staking pool.
    public(friend) fun new(starting_epoch: u64, ctx: &mut TxContext) : StakingPool {
        let exchange_rates = table::new(ctx);
        table::add(
            &mut exchange_rates,
            starting_epoch,
            PoolTokenExchangeRate { sui_amount: 0, pool_token_amount: 0 }
        );
        StakingPool {
            id: object::new(ctx),
            starting_epoch,
            sui_balance: 0,
            rewards_pool: balance::zero(),
            pool_token_balance: 0,
            exchange_rates,
            pending_delegation: 0,
            pending_withdraws: table_vec::empty(ctx),
        }
    }


    // ==== delegation requests ====

    /// Request to delegate to a staking pool. The delegation starts counting at the beginning of the next epoch,
    public(friend) fun request_add_delegation(
        pool: &mut StakingPool,
        stake: Balance<SUI>,
        sui_token_lock: Option<EpochTimeLock>,
        validator_address: address,
        delegator: address,
        ctx: &mut TxContext
    ) {
        let sui_amount = balance::value(&stake);
        assert!(sui_amount > 0, 0);
        let staked_sui = StakedSui {
            id: object::new(ctx),
            pool_id: object::id(pool),
            validator_address,
            delegation_activation_epoch: tx_context::epoch(ctx) + 1,
            principal: stake,
            sui_token_lock,
        };
        pool.pending_delegation = pool.pending_delegation + sui_amount;
        transfer::transfer(staked_sui, delegator);
    }

    /// Request to withdraw `principal_withdraw_amount` of stake plus rewards from a staking pool.
    /// This amount of principal in SUI is withdrawn and transferred to the delegator. A proportional amount
    /// of pool tokens will be later burnt.
    /// The rewards portion will be withdrawn at the end of the epoch, after the rewards have come in so we
    /// can use the new exchange rate to calculate the rewards.
    public(friend) fun request_withdraw_delegation(
        pool: &mut StakingPool,
        staked_sui: StakedSui,
        ctx: &mut TxContext
    ) : u64 {
        let (pool_token_withdraw_amount, principal_withdraw, time_lock) =
            withdraw_from_principal(pool, staked_sui);

        let delegator = tx_context::sender(ctx);
        let principal_withdraw_amount = balance::value(&principal_withdraw);
        table_vec::push_back(&mut pool.pending_withdraws, PendingWithdrawEntry {
            delegator, principal_withdraw_amount, pool_token_withdraw_amount });

        // TODO: implement withdraw bonding period here.
        if (option::is_some(&time_lock)) {
            locked_coin::new_from_balance(principal_withdraw, option::destroy_some(time_lock), delegator, ctx);
        } else {
            transfer::transfer(coin::from_balance(principal_withdraw, ctx), delegator);
            option::destroy_none(time_lock);
        };
        principal_withdraw_amount
    }

    /// Withdraw the principal SUI stored in the StakedSui object, and calculate the corresponding amount of pool
    /// tokens using exchange rate at delegation epoch.
    /// Returns values are amount of pool tokens withdrawn, withdrawn principal portion of SUI, and its
    /// time lock if applicable.
    public(friend) fun withdraw_from_principal(
        pool: &mut StakingPool,
        staked_sui: StakedSui,
    ) : (u64, Balance<SUI>, Option<EpochTimeLock>) {

        // Check that the delegation information matches the pool.
        assert!(staked_sui.pool_id == object::id(pool), EWrongPool);

        let exchange_rate_at_staking_epoch = pool_token_exchange_rate_at_epoch(pool, staked_sui.delegation_activation_epoch);
        let (principal_withdraw, time_lock) = unwrap_staked_sui(staked_sui);
        let pool_token_withdraw_amount = get_token_amount(&exchange_rate_at_staking_epoch, balance::value(&principal_withdraw));

        (
            pool_token_withdraw_amount,
            principal_withdraw,
            time_lock
        )
    }

    fun unwrap_staked_sui(staked_sui: StakedSui): (Balance<SUI>, Option<EpochTimeLock>) {
        let StakedSui {
            id,
            pool_id: _,
            validator_address: _,
            delegation_activation_epoch: _,
            principal,
            sui_token_lock
        } = staked_sui;
        object::delete(id);
        (principal, sui_token_lock)
    }

    // ==== functions called at epoch boundaries ===

    /// Called at epoch advancement times to add rewards (in SUI) to the staking pool.
    public(friend) fun deposit_rewards(pool: &mut StakingPool, rewards: Balance<SUI>, new_epoch: u64) {
        pool.sui_balance = pool.sui_balance + balance::value(&rewards);
        balance::join(&mut pool.rewards_pool, rewards);
        table::add(
            &mut pool.exchange_rates,
            new_epoch,
            PoolTokenExchangeRate { sui_amount: pool.sui_balance, pool_token_amount: pool.pool_token_balance },
        );
    }

    /// Called at epoch boundaries to process pending delegation withdraws requested during the epoch.
    /// For each pending withdraw entry, we withdraw the rewards from the pool at the new exchange rate and burn the pool
    /// tokens.
    public(friend) fun process_pending_delegation_withdraws(pool: &mut StakingPool, ctx: &mut TxContext) : u64 {
        let total_reward_withdraw = 0;
        let new_epoch = tx_context::epoch(ctx) + 1;

        while (!table_vec::is_empty(&pool.pending_withdraws)) {
            let PendingWithdrawEntry {
                delegator, principal_withdraw_amount, pool_token_withdraw_amount
            } = table_vec::pop_back(&mut pool.pending_withdraws);
            let reward_withdraw = withdraw_rewards_and_burn_pool_tokens(
                pool, principal_withdraw_amount, pool_token_withdraw_amount, new_epoch);
            total_reward_withdraw = total_reward_withdraw + balance::value(&reward_withdraw);
            transfer::transfer(coin::from_balance(reward_withdraw, ctx), delegator);
        };
        total_reward_withdraw
    }

    /// Called at epoch boundaries to process the pending delegation.
    public(friend) fun process_pending_delegation(pool: &mut StakingPool, new_epoch: u64) {
        let new_epoch_exchange_rate = pool_token_exchange_rate_at_epoch(pool, new_epoch);
        pool.sui_balance = pool.sui_balance + pool.pending_delegation;
        pool.pool_token_balance = get_token_amount(&new_epoch_exchange_rate, pool.sui_balance);
        pool.pending_delegation = 0;
        check_balance_invariants(pool, new_epoch);
    }

    /// This function does the following:
    ///     1. Calculates the total amount of SUI (including principal and rewards) that the provided pool tokens represent
    ///        at the current exchange rate.
    ///     2. Using the above number and the given `principal_withdraw_amount`, calculates the rewards portion of the
    ///        delegation we should withdraw.
    ///     3. Withdraws the rewards portion from the rewards pool at the current exchange rate. We only withdraw the rewards
    ///        portion because the principal portion was already taken out of the delegator's self custodied StakedSui at request
    ///        time in `request_withdraw_stake`.
    ///     4. Since SUI tokens are withdrawn, we need to burn the corresponding pool tokens to keep the exchange rate the same.
    ///     5. Updates the SUI balance amount of the pool.
    fun withdraw_rewards_and_burn_pool_tokens(
        pool: &mut StakingPool,
        principal_withdraw_amount: u64,
        pool_token_withdraw_amount: u64,
        new_epoch: u64,
    ) : Balance<SUI> {
        let new_epoch_exchange_rate = pool_token_exchange_rate_at_epoch(pool, new_epoch);
        let total_sui_withdraw_amount = get_sui_amount(&new_epoch_exchange_rate, pool_token_withdraw_amount);
        let reward_withdraw_amount =
            if (total_sui_withdraw_amount >= principal_withdraw_amount)
                total_sui_withdraw_amount - principal_withdraw_amount
            else 0;
        // This may happen when we are withdrawing everything from the pool and
        // the rewards pool balance may be less than reward_withdraw_amount.
        // TODO: FIGURE OUT EXACTLY WHY THIS CAN HAPPEN.
        reward_withdraw_amount = math::min(reward_withdraw_amount, balance::value(&pool.rewards_pool));
        pool.sui_balance = pool.sui_balance - (principal_withdraw_amount + reward_withdraw_amount);
        pool.pool_token_balance = pool.pool_token_balance - pool_token_withdraw_amount;
        balance::split(&mut pool.rewards_pool, reward_withdraw_amount)
    }

    // ==== inactive pool related ====

    /// Deactivate a staking pool by wrapping it in an `InactiveStakingPool` and sharing this newly created object.
    /// After this pool deactivation, the pool stops earning rewards. Only delegation withdraws can be made to the pool.
    public(friend) fun deactivate_staking_pool(pool: StakingPool, ctx: &mut TxContext) {
        let inactive_pool = InactiveStakingPool { id: object::new(ctx), pool};
        transfer::share_object(inactive_pool);
    }

    // ==== getters and misc utility functions ====

    public fun sui_balance(pool: &StakingPool) : u64 { pool.sui_balance }

    public fun pool_id(staked_sui: &StakedSui) : ID { staked_sui.pool_id }

    public fun staked_sui_amount(staked_sui: &StakedSui): u64 { balance::value(&staked_sui.principal) }

    public fun delegation_activation_epoch(staked_sui: &StakedSui): u64 {
        staked_sui.delegation_activation_epoch
    }

    public fun pool_token_exchange_rate_at_epoch(pool: &StakingPool, epoch: u64): PoolTokenExchangeRate {
        *table::borrow(&pool.exchange_rates, epoch)
    }
    /// Create a new pending withdraw entry.
    public(friend) fun new_pending_withdraw_entry(
        delegator: address,
        principal_withdraw_amount: u64,
        pool_token_withdraw_amount: u64,
    ) : PendingWithdrawEntry {
        PendingWithdrawEntry { delegator, principal_withdraw_amount, pool_token_withdraw_amount }
    }

    fun get_sui_amount(exchange_rate: &PoolTokenExchangeRate, token_amount: u64): u64 {
        if (exchange_rate.pool_token_amount == 0) {
            return token_amount
        };
        let res = (exchange_rate.sui_amount as u128)
                * (token_amount as u128)
                / (exchange_rate.pool_token_amount as u128);
        (res as u64)
    }

    fun get_token_amount(exchange_rate: &PoolTokenExchangeRate, sui_amount: u64): u64 {
        if (exchange_rate.sui_amount == 0) {
            return sui_amount
        };
        let res = (exchange_rate.pool_token_amount as u128)
                * (sui_amount as u128)
                / (exchange_rate.sui_amount as u128);
        (res as u64)
    }


    fun check_balance_invariants(pool: &StakingPool, epoch: u64) {
        let exchange_rate = pool_token_exchange_rate_at_epoch(pool, epoch);
        // check that the pool token balance and sui balance ratio matches the exchange rate stored.
        let expected = get_token_amount(&exchange_rate, pool.sui_balance);
        let actual = pool.pool_token_balance;
        assert!(expected == actual, ETokenBalancesDoNotMatchExchangeRate)
    }
}
