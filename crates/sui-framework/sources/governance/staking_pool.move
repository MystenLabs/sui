// Copyright (c) Mysten Labs, Inc.
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
    use std::vector;

    friend sui::validator;
    friend sui::validator_set;
    
    const EINSUFFICIENT_POOL_TOKEN_BALANCE: u64 = 0;
    const EWRONG_POOL: u64 = 1;
    const EWITHDRAW_AMOUNT_CANNOT_BE_ZERO: u64 = 2;
    const EINSUFFICIENT_SUI_TOKEN_BALANCE: u64 = 3;
    const EINSUFFICIENT_REWARDS_POOL_BALANCE: u64 = 4;
    const EDESTROY_NON_ZERO_BALANCE: u64 = 5;
    const ETOKEN_TIME_LOCK_IS_SOME: u64 = 6;

    /// A staking pool embedded in each validator struct in the system state object.
    struct StakingPool has store {
        /// The sui address of the validator associated with this pool.
        validator_address: address,
        /// The epoch at which this pool started operating. Should be the epoch at which the validator became active.
        starting_epoch: u64,
        /// The total number of SUI tokens in this pool, including the SUI in the rewards_pool, as well as in all the principal
        /// in the `Delegation` object, updated at epoch boundaries.
        sui_balance: u64,
        /// The epoch delegation rewards will be added here at the end of each epoch. 
        rewards_pool: Balance<SUI>,
        /// The number of delegation pool tokens we have issued so far. This number should equal the sum of
        /// pool token balance in all the `Delegation` objects delegated to this pool. Updated at epoch boundaries.
        delegation_token_supply: Supply<DelegationToken>,
        /// Delegations requested during the current epoch. We will activate these delegation at the end of current epoch
        /// and distribute staking pool tokens at the end-of-epoch exchange rate after the rewards for the current epoch
        /// have been deposited.
        pending_delegations: vector<PendingDelegationEntry>,
        /// Delegation withdraws requested during the current epoch. Similar to new delegation, the withdraws are processed
        /// at epoch boundaries. Rewards are withdrawn and distributed after the rewards for the current epoch have come in. 
        pending_withdraws: vector<PendingWithdrawEntry>,
    }

    /// An inactive staking pool associated with an inactive validator.
    /// Only withdraws can be made from this pool.
    struct InactiveStakingPool has key {
        id: UID, // TODO: inherit an ID from active staking pool?
        pool: StakingPool,
    }

    /// The staking pool token.
    struct DelegationToken has drop {}

    /// Struct representing a pending delegation.
    struct PendingDelegationEntry has store, drop {
        delegator: address, 
        sui_amount: u64,
    }

    /// Struct representing a pending delegation withdraw.
    struct PendingWithdrawEntry has store {
        delegator: address, 
        principal_withdraw_amount: u64,
        withdrawn_pool_tokens: Balance<DelegationToken>,
    }

    /// A self-custodial delegation object, serving as evidence that the delegator
    /// has delegated to a staking pool.
    struct Delegation has key {
        id: UID,
        /// The sui address of the validator associated with the staking pool this object delgates to.
        validator_address: address,
        /// The epoch at which the staking pool started operating.
        pool_starting_epoch: u64,
        /// The pool tokens representing the amount of rewards the delegator can get back when they withdraw
        /// from the pool. If this field is `none`, that means the delegation hasn't been activated yet.
        pool_tokens: Balance<DelegationToken>,
        /// Number of SUI token staked originally.
        principal_sui_amount: u64,
    }

    /// A self-custodial object holding the staked SUI tokens.
    struct StakedSui has key {
        id: UID,
        /// The staked SUI tokens.
        principal: Balance<SUI>,
        /// If the stake comes from a Coin<SUI>, this field is None. If it comes from a LockedCoin<SUI>, this
        /// field will record the original lock expiration epoch, to be used when unstaking.
        sui_token_lock: Option<EpochTimeLock>,
    }

    // == initializer ==

    /// Create a new, empty staking pool.
    public(friend) fun new(validator_address: address, starting_epoch: u64) : StakingPool {
        StakingPool {
            validator_address,
            starting_epoch,
            sui_balance: 0,
            rewards_pool: balance::zero(),
            delegation_token_supply: balance::create_supply(DelegationToken {}),
            pending_delegations: vector::empty(),
            pending_withdraws: vector::empty(),
        }
    }


    // == delegation requests ==

    // TODO: implement rate limiting new delegations per epoch.
    /// Request to delegate to a staking pool. The delegation gets counted at the beginning of the next epoch,
    /// when the delegation object containing the pool tokens is distributed to the delegator.
    public(friend) fun request_add_delegation(
        pool: &mut StakingPool, 
        stake: Balance<SUI>, 
        sui_token_lock: Option<EpochTimeLock>,
        delegator: address,
        ctx: &mut TxContext
    ) {
        let sui_amount = balance::value(&stake);
        assert!(sui_amount > 0, 0);
        // insert delegation info into the pendng_delegations vector.         
        vector::push_back(&mut pool.pending_delegations, PendingDelegationEntry { delegator, sui_amount });
        let staked_sui = StakedSui {
            id: object::new(ctx),
            principal: stake,
            sui_token_lock,
        };
        transfer::transfer(staked_sui, delegator);
    }

    /// Activate a delegation. New pool tokens are minted at the current exchange rate and put into the
    /// `pool_tokens` field of the delegation object.
    /// After activation, the delegation officially counts toward the staking power of the validator.
    /// Aborts if the pool mismatches, the delegation is already activated, or the delegation cannot be activated yet. 
    public(friend) fun mint_delegation_tokens_to_delegator(
        pool: &mut StakingPool, 
        delegator: address, 
        sui_amount: u64, 
        ctx: &mut TxContext
    ) {
        let new_pool_token_amount = get_token_amount(pool, sui_amount);   

        // Mint new pool tokens at the current exchange rate.
        let pool_tokens = balance::increase_supply(&mut pool.delegation_token_supply, new_pool_token_amount);

        let delegation = Delegation {
            id: object::new(ctx),
            validator_address: pool.validator_address,
            pool_starting_epoch: pool.starting_epoch,
            pool_tokens,
            principal_sui_amount: sui_amount,
        };

        transfer::transfer(delegation, delegator);
    }

    /// Withdraw `withdraw_pool_token_amount` worth of delegated stake from a staking pool. A proportional amount of principal
    /// in SUI will be withdrawn and transferred to the delegator. The rewards portion is withdrawn at the end of the epoch.
    /// Returns the amount of SUI withdrawn.
    public(friend) fun request_withdraw_stake(
        pool: &mut StakingPool,  
        delegation: &mut Delegation, 
        staked_sui: &mut StakedSui,
        withdraw_pool_token_amount: u64, 
        ctx: &mut TxContext
    ) : u64 {
        let (withdrawn_pool_tokens, principal_withdraw, time_lock) = 
            withdraw_principal(pool, delegation, staked_sui, withdraw_pool_token_amount);
        
        let principal_withdraw_amount = balance::value(&principal_withdraw);

        let delegator = tx_context::sender(ctx);
        vector::push_back(&mut pool.pending_withdraws, PendingWithdrawEntry { 
            delegator, principal_withdraw_amount, withdrawn_pool_tokens });

        // TODO: implement withdraw bonding period here.
        if (option::is_some(&time_lock)) {
            locked_coin::new_from_balance(principal_withdraw, option::destroy_some(time_lock), delegator, ctx);
        } else {
            transfer::transfer(coin::from_balance(principal_withdraw, ctx), delegator);
            option::destroy_none(time_lock);
        };
        principal_withdraw_amount
    }

    /// Withdraw a proportional amount of the principal SUI stored in the StakedSui object.
    public(friend) fun withdraw_principal(
        pool: &mut StakingPool,  
        delegation: &mut Delegation, 
        staked_sui: &mut StakedSui,
        withdraw_pool_token_amount: u64,
    ) : (Balance<DelegationToken>, Balance<SUI>, Option<EpochTimeLock>) {
        assert!(
            delegation.validator_address == pool.validator_address &&
            delegation.pool_starting_epoch == pool.starting_epoch,
            EWRONG_POOL
        );

        assert!(withdraw_pool_token_amount > 0, EWITHDRAW_AMOUNT_CANNOT_BE_ZERO);

        let pool_token_balance = balance::value(&delegation.pool_tokens);
        assert!(pool_token_balance >= withdraw_pool_token_amount, EINSUFFICIENT_POOL_TOKEN_BALANCE);

        // Calculate the amounts of SUI to be withdrawn from the principal component.
        // We already checked that pool_token_balance is greater than zero.
        let sui_withdraw_from_principal = 
            (delegation.principal_sui_amount as u128) * (withdraw_pool_token_amount as u128) / (pool_token_balance as u128);
        
        let (principal_withdraw, time_lock) = withdraw_from_principal(delegation, staked_sui, (sui_withdraw_from_principal as u64));

        (
            balance::split(&mut delegation.pool_tokens, withdraw_pool_token_amount),
            principal_withdraw,
            time_lock
        )
    }


    // == functions called at epoch boundaries ==

    /// Called at epoch advancement times to add rewards (in SUI) to the staking pool, and process pending withdraws. 
    public(friend) fun distribute_rewards(pool: &mut StakingPool, rewards: Balance<SUI>, ctx: &mut TxContext): u64 {
        pool.sui_balance = pool.sui_balance + balance::value(&rewards);
        balance::join(&mut pool.rewards_pool, rewards);
        let total_reward_withdraw = 0;

        while (!vector::is_empty(&pool.pending_withdraws)) {
            let PendingWithdrawEntry { delegator, principal_withdraw_amount, withdrawn_pool_tokens } = vector::pop_back(&mut pool.pending_withdraws);
            let reward_withdraw = withdraw_rewards_and_burn_pool_tokens(pool, principal_withdraw_amount, withdrawn_pool_tokens);
            total_reward_withdraw = total_reward_withdraw + balance::value(&reward_withdraw);
            transfer::transfer(coin::from_balance(reward_withdraw, ctx), delegator);
        };
        total_reward_withdraw
    }

    /// Called at epoch boundaries to mint new pool tokens to new delegators at the new exchange rate.
    public(friend) fun process_pending_delegations(pool: &mut StakingPool, ctx: &mut TxContext) : u64 {
        let before_sui_balance = pool.sui_balance;
        while (!vector::is_empty(&pool.pending_delegations)) {
            let PendingDelegationEntry { delegator, sui_amount } = vector::pop_back(&mut pool.pending_delegations);
            mint_delegation_tokens_to_delegator(pool, delegator, sui_amount, ctx);
            pool.sui_balance = pool.sui_balance + sui_amount;
        };
        pool.sui_balance - before_sui_balance
    }

    /// Called by validator_set at epoch boundaries for delegation switches.
    public(friend) fun batch_rewards_withdraws(
        pool: &mut StakingPool,
        entries: vector<PendingWithdrawEntry>,
    ) : (vector<address>, vector<Balance<SUI>>, u64) {
        let (delegators, rewards, total_rewards_withdraw_amount) = (vector::empty(), vector::empty(), 0);
        while (!vector::is_empty(&mut entries)) {
            let PendingWithdrawEntry { delegator, principal_withdraw_amount, withdrawn_pool_tokens } 
                = vector::pop_back(&mut entries);
            let reward = withdraw_rewards_and_burn_pool_tokens(pool, principal_withdraw_amount, withdrawn_pool_tokens);
            total_rewards_withdraw_amount = total_rewards_withdraw_amount + balance::value(&reward);
            vector::push_back(&mut delegators, delegator);
            vector::push_back(&mut rewards, reward);
        };
        vector::destroy_empty(entries);
        (delegators, rewards, total_rewards_withdraw_amount)
    }

    public(friend) fun withdraw_rewards_and_burn_pool_tokens(
        pool: &mut StakingPool, 
        principal_withdraw_amount: u64, 
        withdrawn_pool_tokens: Balance<DelegationToken>,
    ) : Balance<SUI> {
        let pool_token_amount = balance::value(&withdrawn_pool_tokens);
        let total_sui_withdraw_amount = get_sui_amount(pool, pool_token_amount);
        assert!(total_sui_withdraw_amount >= principal_withdraw_amount, 0);
        let reward_withdraw_amount = total_sui_withdraw_amount - principal_withdraw_amount;
        balance::decrease_supply(
            &mut pool.delegation_token_supply, 
            withdrawn_pool_tokens
        );
        pool.sui_balance = pool.sui_balance - (principal_withdraw_amount + reward_withdraw_amount);
        balance::split(&mut pool.rewards_pool, reward_withdraw_amount)
    }

    // == inactive pool related ==

    /// Deactivate a staking pool by wrapping it in an `InactiveStakingPool` and sharing this newly created object. 
    /// After this pool deactivation, the pool stops earning rewards. Only delegation withdraws can be made to the pool.
    public(friend) fun deactivate_staking_pool(pool: StakingPool, ctx: &mut TxContext) {
        let inactive_pool = InactiveStakingPool { id: object::new(ctx), pool};
        transfer::share_object(inactive_pool);
    }

    /// Withdraw delegation from an inactive pool.
    public entry fun withdraw_from_inactive_pool(
        inactive_pool: &mut InactiveStakingPool, 
        staked_sui: &mut StakedSui, 
        delegation: &mut Delegation, 
        withdraw_pool_token_amount: u64, 
        ctx: &mut TxContext
    ) {
        let pool = &mut inactive_pool.pool;
        let (withdrawn_pool_tokens, principal_withdraw, time_lock) = 
            withdraw_principal(pool, delegation, staked_sui, withdraw_pool_token_amount);
        let principal_withdraw_amount = balance::value(&principal_withdraw);
        let rewards_withdraw = withdraw_rewards_and_burn_pool_tokens(pool, principal_withdraw_amount, withdrawn_pool_tokens);
        let total_withdraw_amount = principal_withdraw_amount + balance::value(&rewards_withdraw);
        pool.sui_balance = pool.sui_balance - total_withdraw_amount;

        let delegator = tx_context::sender(ctx);
        // TODO: implement withdraw bonding period here.
        if (option::is_some(&time_lock)) {
            locked_coin::new_from_balance(principal_withdraw, option::destroy_some(time_lock), delegator, ctx);
            transfer::transfer(coin::from_balance(rewards_withdraw, ctx), delegator);
        } else {
            balance::join(&mut principal_withdraw, rewards_withdraw);
            transfer::transfer(coin::from_balance(principal_withdraw, ctx), delegator);
            option::destroy_none(time_lock);
        };
    }

    /// Destroy an empty delegation that no longer contains any SUI or pool tokens.
    public entry fun destroy_empty_delegation(delegation: Delegation) {
        let Delegation {
            id,
            validator_address: _,
            pool_starting_epoch: _,
            pool_tokens,
            principal_sui_amount,
        } = delegation;
        object::delete(id);
        assert!(balance::value(&pool_tokens) == 0, EDESTROY_NON_ZERO_BALANCE);
        assert!(principal_sui_amount == 0, EDESTROY_NON_ZERO_BALANCE);
        balance::destroy_zero(pool_tokens);
    }

    /// Destroy an empty delegation that no longer contains any SUI or pool tokens.
    public entry fun destroy_empty_staked_sui(staked_sui: StakedSui) {
        let StakedSui {
            id,
            principal,
            sui_token_lock
        } = staked_sui;
        object::delete(id);
        assert!(balance::value(&principal) == 0, EDESTROY_NON_ZERO_BALANCE);
        balance::destroy_zero(principal);
        assert!(option::is_none(&sui_token_lock), ETOKEN_TIME_LOCK_IS_SOME);
        option::destroy_none(sui_token_lock);
    }


    // == getters and misc utility functions ==

    public fun sui_balance(pool: &StakingPool) : u64 { pool.sui_balance }

    public fun validator_address(delegation: &Delegation) : address { delegation.validator_address }

    public fun staked_sui_amount(staked_sui: &StakedSui): u64 { balance::value(&staked_sui.principal) }

    public fun delegation_token_amount(delegation: &Delegation): u64 { balance::value(&delegation.pool_tokens) }

    public(friend) fun new_pending_withdraw_entry(
        delegator: address, 
        principal_withdraw_amount: u64,
        withdrawn_pool_tokens: Balance<DelegationToken>,
    ) : PendingWithdrawEntry {
        PendingWithdrawEntry { delegator, principal_withdraw_amount, withdrawn_pool_tokens }
    }

    /// Withdraw `withdraw_amount` of SUI tokens from the delegation and give it back to the delegator
    /// in the original state of the tokens.
    fun withdraw_from_principal(
        delegation: &mut Delegation, 
        staked_sui: &mut StakedSui, 
        withdraw_amount: u64,
    ) : (Balance<SUI>, Option<EpochTimeLock>) {
        assert!(balance::value(&staked_sui.principal) >= withdraw_amount, EINSUFFICIENT_SUI_TOKEN_BALANCE);
        delegation.principal_sui_amount = delegation.principal_sui_amount - withdraw_amount;
        let principal_withdraw = balance::split(&mut staked_sui.principal, withdraw_amount);
        if (option::is_some(&staked_sui.sui_token_lock)) {
            let time_lock = 
                if (balance::value(&staked_sui.principal) == 0) {option::extract(&mut staked_sui.sui_token_lock)}
                else *option::borrow(&staked_sui.sui_token_lock);
            (principal_withdraw, option::some(time_lock))
        } else {
            (principal_withdraw, option::none())
        }
    }

    fun get_sui_amount(pool: &StakingPool, token_amount: u64): u64 {
        let token_supply = balance::supply_value(&pool.delegation_token_supply);
        if (token_supply == 0) { 
            return token_amount 
        };
        let res = (pool.sui_balance as u128) 
                * (token_amount as u128) 
                / (token_supply as u128);
        (res as u64)
    }

    fun get_token_amount(pool: &StakingPool, sui_amount: u64): u64 {
        if (pool.sui_balance == 0) { 
            return sui_amount
        };
        let token_supply = balance::supply_value(&pool.delegation_token_supply);
        let res = (token_supply as u128) 
                * (sui_amount as u128)
                / (pool.sui_balance as u128);
        (res as u64)
    }    
}
