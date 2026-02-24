// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Simple token staking module with rewards.
///
/// Users can stake tokens and earn rewards over time.
/// Rewards are calculated based on staking duration and amount.
/// Users can unstake and claim their rewards at any time.
module staking::simple_staking {
    use sui::object::{Self, Info, UID};
    use sui::coin::{Self, Coin};
    use sui::balance::{Self, Balance};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    /// Error codes
    const EInvalidAmount: u64 = 0;
    const ENoStake: u64 = 1;
    const EInsufficientRewards: u64 = 2;

    /// Reward rate per epoch (in basis points, 100 = 1%)
    const REWARD_RATE: u64 = 100; // 1% per epoch

    /// A staking pool that holds staked tokens and distributes rewards
    struct StakingPool<phantom T> has key {
        id: UID,
        staked_balance: Balance<T>,
        reward_balance: Balance<T>,
        total_staked: u64,
    }

    /// A stake position owned by a user
    struct StakePosition<phantom T> has key {
        id: UID,
        owner: address,
        amount: u64,
        start_epoch: u64,
        last_claim_epoch: u64,
    }

    /// Initialize a new staking pool
    public entry fun create_pool<T>(
        initial_rewards: Coin<T>,
        ctx: &mut TxContext
    ) {
        let pool = StakingPool<T> {
            id: object::new(ctx),
            staked_balance: balance::zero(),
            reward_balance: coin::into_balance(initial_rewards),
            total_staked: 0,
        };

        transfer::share_object(pool);
    }

    /// Stake tokens
    public entry fun stake<T>(
        pool: &mut StakingPool<T>,
        stake_coin: Coin<T>,
        ctx: &mut TxContext
    ) {
        let amount = coin::value(&stake_coin);
        assert!(amount > 0, EInvalidAmount);

        // Add to pool
        let stake_balance = coin::into_balance(stake_coin);
        balance::join(&mut pool.staked_balance, stake_balance);
        pool.total_staked = pool.total_staked + amount;

        // Create stake position
        let position = StakePosition<T> {
            id: object::new(ctx),
            owner: tx_context::sender(ctx),
            amount,
            start_epoch: tx_context::epoch(ctx),
            last_claim_epoch: tx_context::epoch(ctx),
        };

        transfer::transfer(position, tx_context::sender(ctx));
    }

    /// Unstake tokens and claim rewards
    public entry fun unstake<T>(
        pool: &mut StakingPool<T>,
        position: StakePosition<T>,
        ctx: &mut TxContext
    ) {
        let StakePosition {
            id,
            owner,
            amount,
            start_epoch: _,
            last_claim_epoch
        } = position;

        assert!(tx_context::sender(ctx) == owner, ENoStake);

        // Calculate rewards
        let current_epoch = tx_context::epoch(ctx);
        let epochs_staked = current_epoch - last_claim_epoch;
        let rewards = calculate_rewards(amount, epochs_staked);

        // Ensure pool has enough rewards
        assert!(balance::value(&pool.reward_balance) >= rewards, EInsufficientRewards);

        // Withdraw staked tokens
        let staked = coin::take(&mut pool.staked_balance, amount, ctx);
        pool.total_staked = pool.total_staked - amount;

        // Withdraw rewards
        let reward_coin = coin::take(&mut pool.reward_balance, rewards, ctx);

        // Transfer tokens and rewards to user
        transfer::transfer(staked, owner);
        transfer::transfer(reward_coin, owner);

        object::delete(id);
    }

    /// Claim rewards without unstaking
    public entry fun claim_rewards<T>(
        pool: &mut StakingPool<T>,
        position: &mut StakePosition<T>,
        ctx: &mut TxContext
    ) {
        assert!(tx_context::sender(ctx) == position.owner, ENoStake);

        // Calculate rewards
        let current_epoch = tx_context::epoch(ctx);
        let epochs_staked = current_epoch - position.last_claim_epoch;
        let rewards = calculate_rewards(position.amount, epochs_staked);

        // Ensure pool has enough rewards
        assert!(balance::value(&pool.reward_balance) >= rewards, EInsufficientRewards);

        // Update last claim epoch
        position.last_claim_epoch = current_epoch;

        // Withdraw and transfer rewards
        let reward_coin = coin::take(&mut pool.reward_balance, rewards, ctx);
        transfer::transfer(reward_coin, position.owner);
    }

    /// Add rewards to the pool
    public entry fun add_rewards<T>(
        pool: &mut StakingPool<T>,
        rewards: Coin<T>,
        _ctx: &mut TxContext
    ) {
        let reward_balance = coin::into_balance(rewards);
        balance::join(&mut pool.reward_balance, reward_balance);
    }

    /// Helper function to calculate rewards
    fun calculate_rewards(amount: u64, epochs: u64): u64 {
        // Simple linear rewards: amount * rate * epochs / 10000
        let base_reward = ((amount as u128) * (REWARD_RATE as u128) * (epochs as u128)) / 10000;
        (base_reward as u64)
    }

    /// View functions

    /// Get stake position info
    public fun get_stake_info<T>(position: &StakePosition<T>): (address, u64, u64, u64) {
        (position.owner, position.amount, position.start_epoch, position.last_claim_epoch)
    }

    /// Get pool info
    public fun get_pool_info<T>(pool: &StakingPool<T>): (u64, u64, u64) {
        (
            balance::value(&pool.staked_balance),
            balance::value(&pool.reward_balance),
            pool.total_staked
        )
    }

    /// Calculate pending rewards for a position
    public fun pending_rewards<T>(position: &StakePosition<T>, current_epoch: u64): u64 {
        let epochs_staked = current_epoch - position.last_claim_epoch;
        calculate_rewards(position.amount, epochs_staked)
    }
}
