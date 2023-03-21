// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::staking_pool {
    use sui::balance::{Self, Balance};
    use sui::sui::SUI;
    use std::option::{Self, Option};
    use sui::tx_context::TxContext;
    use sui::object::{Self, ID, UID};
    use sui::table::{Self, Table};
    use sui::bag::Bag;
    use sui::bag;
    use sui::transfer;
    use sui::tx_context;

    friend sui::validator;
    friend sui::validator_set;

    /// A staking pool embedded in each validator struct in the system state object.
    struct StakingPool has key, store {
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
    struct PoolTokenExchangeRate has store, copy, drop {
        sui_amount: u64,
        pool_token_amount: u64,
    }

    /// A self-custodial object holding the staked SUI tokens.
    struct StakedSui has key {
        id: UID,
        /// ID of the staking pool we are staking with.
        pool_id: ID,
        // TODO: keeping this field here because the apps depend on it. consider removing it.
        validator_address: address,
        /// The epoch at which the stake becomes active.
        stake_activation_epoch: u64,
        /// The staked SUI tokens.
        principal: Balance<SUI>,
    }

    // ==== initializer ====

    /// Create a new, empty staking pool.
    public(friend) fun new(init_stake: Balance<SUI>, ctx: &mut TxContext) : StakingPool {
        let exchange_rates = table::new(ctx);
        let sui_amount = balance::value(&init_stake);
        let pool = StakingPool {
            id: object::new(ctx),
            activation_epoch: option::some(tx_context::epoch(ctx)),
            deactivation_epoch: option::none(),
            sui_balance: sui_amount,
            rewards_pool: balance::zero(),
            pool_token_balance: 0,
            exchange_rates,
            pending_stake: 0,
            pending_total_sui_withdraw: 0,
            pending_pool_token_withdraw: 0,
            extra_fields: bag::new(ctx),
        };
        // We don't care about who owns the staked sui in the mock test.
        let staked_sui = StakedSui {
            id: object::new(ctx),
            pool_id: object::id(&pool),
            validator_address: tx_context::sender(ctx),
            stake_activation_epoch: tx_context::epoch(ctx),
            principal: init_stake,
        };
        transfer::transfer(staked_sui, tx_context::sender(ctx));
        pool
    }
}
