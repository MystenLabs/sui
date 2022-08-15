// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::staking_pool {
    use sui::balance::{Self, Balance, Supply};
    use sui::sui::SUI;
    use std::option::{Self, Option};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer;
    use sui::epoch_time_lock::{EpochTimeLock};
    use sui::object::{Self, UID};
    use sui::locked_coin;
    use sui::coin;

    friend sui::validator;

    const DELEGATION_ALREADY_ACTIVATED: u64 = 0; 
    const DELEGATION_NOT_YET_ACTIVATED: u64 = 1;
    const CANNOT_ACTIVATE_DELEGATION: u64 = 2;
    const INSUFFICIENT_POOL_TOKEN_BALANCE: u64 = 3;
    const WRONG_POOL: u64 = 4;

    /// A staking pool embedded in each validator struct in the system state object.
    struct StakingPool has store {
        /// The sui address of the validator associated with this pool.
        validator_address: address,
        /// The epoch at which this pool started operating. Should be the epoch at which the validator became active.
        starting_epoch: u64,
        /// The total number of SUI tokens in this pool, including the SUI in the rewards_pool, as well as in all the principal
        /// in the `Delegation` object.
        sui_balance: u64,
        /// The epoch delegation rewards will be added here at the end of each epoch. 
        rewards_pool: Balance<SUI>,
        /// The number of delegation pool tokens we have issued so far. This number should equal the sum of
        /// pool token balance in all the `Delegation` objects delegated to this pool.
        delegation_token_supply: Supply<DelegationToken>,
        // TODO: add fields for implementing bonding periods if necessary
    }

    /// An inactive staking pool associated with an inactive validator.
    /// Only withdraws can be made from this pool.
    struct InactiveStakingPool has key {
        id: UID,
        pool: StakingPool,
    }

    /// The staking pool token.
    struct DelegationToken has drop {}

    /// A self-custodial delegation object, serving as evidence that the delegator
    /// has delegated to a staking pool.
    struct Delegation has key {
        id: UID,
        /// The sui address of the validator associated with the staking pool this object delgates to.
        validator_address: address,
        /// The epoch at which the staking pool started operating.
        pool_starting_epoch: u64,
        /// The ealiest epoch at which this delegation can be activated.
        earliest_activation_epoch: u64,
        /// The staked SUI tokens.
        principal: Balance<SUI>,
        /// If the stake comes from a Coin<SUI>, this field is None. If it comes from a LockedCoin<SUI>, this
        /// field will record the original lock expiration epoch, to be used when unstaking.
        sui_token_lock: Option<EpochTimeLock>,
        /// The pool tokens representing the amount of rewards the delegator can get back when they withdraw
        /// from the pool. If this field is `none`, that means the delegation hasn't been activated yet.
        pool_tokens: Option<Balance<DelegationToken>>,
    }

    /// Create a new, empty staking pool.
    public(friend) fun new(validator_address: address, starting_epoch: u64) : StakingPool {
        StakingPool {
            validator_address,
            starting_epoch,
            sui_balance: 0,
            rewards_pool: balance::zero(),
            delegation_token_supply: balance::create_supply(DelegationToken {}),
        }
    }

    /// Add rewards (in SUI) to the staking pool. 
    /// Notice here we add SUI tokens but do not mint new pool tokens. Thus the exchange rate between SUI and pool tokens goes up.
    public(friend) fun add_rewards(pool: &mut StakingPool, rewards: Balance<SUI>) {
        pool.sui_balance = pool.sui_balance + balance::value(&rewards);
        balance::join(&mut pool.rewards_pool, rewards);
        // TODO: implement potential bonding period related bookkeeping
    }

    /// Request to delegate to a staking pool. The delegation doesn't get activated and the delegator doesn't get
    /// the pool tokens until they activate the delegation on or after the `earliest_activation_epoch`.
    public(friend) fun request_add_delegation(pool: &mut StakingPool, stake: Balance<SUI>, sui_token_lock: Option<EpochTimeLock>, ctx: &mut TxContext) {
        assert!(balance::value(&stake) > 0, 0);
        let delegation = Delegation {
            id: object::new(ctx),
            validator_address: pool.validator_address,
            pool_starting_epoch: pool.starting_epoch,
            earliest_activation_epoch: tx_context::epoch(ctx) + 1, // TODO: with bonding period this should be the epoch
                                                                   // at which the bonding period ends
            principal: stake,
            sui_token_lock,
            pool_tokens: option::none(),
        };
        transfer::transfer(delegation, tx_context::sender(ctx));
    }

    /// Activate a delegation. New pool tokens are minted at the current exchange rate and put into the
    /// `pool_tokens` field of the delegation object.
    /// After activation, the delegation officially counts toward the staking power of the validator.
    /// Aborts if the pool mismatches, the delegation is already activated, or the delegation cannot be activated yet. 
    public(friend) fun activate_delegation(pool: &mut StakingPool, delegation: &mut Delegation, ctx: &mut TxContext) {
        assert!(
            delegation.validator_address == pool.validator_address &&
            delegation.pool_starting_epoch == pool.starting_epoch,
            WRONG_POOL
        );
        assert!(!is_delegation_activated(delegation), DELEGATION_ALREADY_ACTIVATED);

        assert!(tx_context::epoch(ctx) >= delegation.earliest_activation_epoch, CANNOT_ACTIVATE_DELEGATION); 

        let sui_amount = balance::value(&delegation.principal);
        let new_pool_token_amount = get_token_amount(pool, sui_amount);   

        // Mint new pool tokens at the current exchange rate.
        let pool_tokens = balance::increase_supply(&mut pool.delegation_token_supply, new_pool_token_amount);

        // Put the newly minted pool tokens into the delegation object to activate it.
        option::fill(&mut delegation.pool_tokens, pool_tokens);

        pool.sui_balance = pool.sui_balance + sui_amount;
    }

    /// Withdraw `withdraw_pool_token_amount` worth of delegated stake from a staking pool. A proportional amount of principal and rewards
    /// in SUI will be withdrawn and transferred back to the delegator. 
    public(friend) fun withdraw_stake(pool: &mut StakingPool, delegation: &mut Delegation, withdraw_pool_token_amount: u64, ctx: &mut TxContext) {
        assert!(
            delegation.validator_address == pool.validator_address &&
            delegation.pool_starting_epoch == pool.starting_epoch,
            WRONG_POOL
        );
        assert!(is_delegation_activated(delegation), DELEGATION_NOT_YET_ACTIVATED);

        let pool_tokens_ref = option::borrow_mut(&mut delegation.pool_tokens);
        let pool_token_balance = balance::value(pool_tokens_ref);
        assert!(pool_token_balance >= withdraw_pool_token_amount, INSUFFICIENT_POOL_TOKEN_BALANCE);

        // Calculate the amount of SUI tokens that should be withdrawn from the pool using the current exchange rate.
        let sui_withdraw_amount = get_sui_amount(pool, withdraw_pool_token_amount);

        let sui_principal_amount = balance::value(&delegation.principal);

        // Calculate the amounts if SUI to be withdrawn from the principal component and the rewards component.
        let sui_withdraw_from_principal = (sui_principal_amount as u128) * (withdraw_pool_token_amount as u128) / (pool_token_balance as u128);
        let sui_withdraw_from_rewards = sui_withdraw_amount - (sui_withdraw_from_principal as u64); 

        // burn the pool tokens
        balance::decrease_supply(
            &mut pool.delegation_token_supply, 
            balance::split(pool_tokens_ref, withdraw_pool_token_amount)
        );

        // TODO: implement withdraw bonding period here.
        withdraw_from_principal(delegation, (sui_withdraw_from_principal as u64), ctx);

        let reward_withdraw = balance::split(&mut pool.rewards_pool, sui_withdraw_from_rewards);
        // TODO: implement withdraw bonding period here.
        transfer::transfer(coin::from_balance(reward_withdraw, ctx), tx_context::sender(ctx));

        // decrement sui balance in the pool
        pool.sui_balance = pool.sui_balance - sui_withdraw_amount;
    }

    /// Deactivate a staking pool by wrapping it in an `InactiveStakingPool` and sharing this newly created object. 
    /// After this pool deactivation, the pool stops earning rewards. Only delegation withdraws can be made to the pool.
    public(friend) fun deactivate_staking_pool(pool: StakingPool, ctx: &mut TxContext) {
        let inactive_pool = InactiveStakingPool { id: object::new(ctx), pool};
        // TODO: emit events here with the id of the inactive pool.
        transfer::share_object(inactive_pool);
    }

    /// Withdraw delegation from an inactive pool.
    public entry fun withdraw_from_inactive_pool(
        inactive_pool: &mut InactiveStakingPool, 
        delegation: &mut Delegation, 
        withdraw_amount: u64, 
        ctx: &mut TxContext
    ) {
        withdraw_stake(&mut inactive_pool.pool, delegation, withdraw_amount, ctx);
    }

    /// Destroy an empty delegation that no longer contains any SUI or pool tokens.
    public entry fun destroy_empty_delegation(delegation: Delegation) {
        let Delegation {
            id,
            validator_address: _,
            pool_starting_epoch: _,
            earliest_activation_epoch: _,
            principal,
            sui_token_lock,
            pool_tokens,
        } = delegation;
        object::delete(id);
        balance::destroy_zero(principal);
        option::destroy_none(sui_token_lock);
        if (option::is_some(&pool_tokens)) {
            balance::destroy_zero(option::extract(&mut pool_tokens));
        };
        option::destroy_none(pool_tokens);
    }

    public fun sui_balance(pool: &StakingPool) : u64 { pool.sui_balance }

    public fun validator_address(delegation: &Delegation) : address { delegation.validator_address }

    public fun delegation_sui_amount(delegation: &Delegation): u64 { balance::value(&delegation.principal) }

    public fun is_delegation_activated(delegation: &Delegation) : bool {
        option::is_some(&delegation.pool_tokens)
    }

    /// Withdraw `withdraw_amount` of SUI tokens from the delegation and give it back to the delegator
    /// in the original state of the tokens.
    fun withdraw_from_principal(delegation: &mut Delegation, withdraw_amount: u64, ctx: &mut TxContext) {
        let delegator = tx_context::sender(ctx);
        let principal_withdraw = balance::split(&mut delegation.principal, withdraw_amount);
        if (option::is_some(&delegation.sui_token_lock)) {
            let time_lock = 
                if (balance::value(&delegation.principal) == 0) {option::extract(&mut delegation.sui_token_lock)}
                else *option::borrow(&delegation.sui_token_lock);
            locked_coin::new_from_balance(principal_withdraw, time_lock, delegator, ctx);

        } else {
            transfer::transfer(coin::from_balance(principal_withdraw, ctx), delegator);
        };
    }

    fun get_sui_amount(pool: &StakingPool, token_amount: u64): u64 {
        let token_supply_amount = balance::supply_value(&pool.delegation_token_supply);
        if (token_supply_amount == 0) { 
            return token_amount 
        };
        let res = (pool.sui_balance as u128) * (token_amount as u128) / (token_supply_amount as u128);
        (res as u64)
    }

    fun get_token_amount(pool: &StakingPool, sui_amount: u64): u64 {
        let token_supply_amount = balance::supply_value(&pool.delegation_token_supply);
        if (pool.sui_balance == 0) { 
            return sui_amount
        };
        let res = (token_supply_amount as u128) * (sui_amount as u128) / (pool.sui_balance as u128);
        (res as u64)
    }    
}
