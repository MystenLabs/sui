// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::sui_system {
    use sui::balance::{Self, Balance, Supply};
    use sui::coin::{Self, Coin};
    use sui::staking_pool::{Delegation, StakedSui};
    use sui::object::{Self, UID};
    use sui::locked_coin::{Self, LockedCoin};
    use sui::sui::SUI;
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use sui::validator::{Self, Validator};
    use sui::validator_set::{Self, ValidatorSet};
    use sui::stake::Stake;
    use sui::stake_subsidy::{Self, StakeSubsidy};
    use sui::vec_map::{Self, VecMap};
    use sui::vec_set::{Self, VecSet};
    use std::option;

    friend sui::genesis;

    #[test_only]
    friend sui::governance_test_utils;

    /// A list of system config parameters.
    // TDOO: We will likely add more, a few potential ones:
    // - the change in stake across epochs can be at most +/- x%
    // - the change in the validator set across epochs can be at most x validators
    //
    // TODO: The stake threshold should be % threshold instead of amount threshold.
    struct SystemParameters has store {
        /// Lower-bound on the amount of stake required to become a validator.
        min_validator_stake: u64,
        /// Maximum number of validator candidates at any moment.
        /// We do not allow the number of validators in any epoch to go above this.
        max_validator_candidate_count: u64,
        /// Storage gas price denominated in SUI
        storage_gas_price: u64,
    }

    /// The top-level object containing all information of the Sui system.
    struct SuiSystemState has key {
        id: UID,
        /// Id of the chain, value in the range [1, 127].
        chain_id: u8,
        /// The current epoch ID, starting from 0.
        epoch: u64,
        /// Contains all information about the validators.
        validators: ValidatorSet,
        /// The SUI treasury capability needed to mint SUI.
        sui_supply: Supply<SUI>,
        /// The storage fund.
        storage_fund: Balance<SUI>,
        /// A list of system config parameters.
        parameters: SystemParameters,
        /// The reference gas price for the current epoch.
        reference_gas_price: u64,
        /// A map storing the records of validator reporting each other during the current epoch. 
        /// There is an entry in the map for each validator that has been reported
        /// at least once. The entry VecSet contains all the validators that reported
        /// them. If a validator has never been reported they don't have an entry in this map.
        /// This map resets every epoch.
        validator_report_records: VecMap<address, VecSet<address>>,
        /// Schedule of stake subsidies given out each epoch.
        stake_subsidy: StakeSubsidy,
    }

    // Errors
    const ENOT_VALIDATOR: u64 = 0;
    const ELIMIT_EXCEEDED: u64 = 1;
    const EEPOCH_NUMBER_MISMATCH: u64 = 2;
    const ECANNOT_REPORT_ONESELF: u64 = 3;
    const EREPORT_RECORD_NOT_FOUND: u64 = 4;

    const BASIS_POINT_DENOMINATOR: u128 = 10000;

    // ==== functions that can only be called by genesis ====

    /// Create a new SuiSystemState object and make it shared.
    /// This function will be called only once in genesis.
    public(friend) fun create(
        chain_id: u8,
        validators: vector<Validator>,
        sui_supply: Supply<SUI>,
        storage_fund: Balance<SUI>,
        max_validator_candidate_count: u64,
        min_validator_stake: u64,
        storage_gas_price: u64,
        initial_stake_subsidy_amount: u64,
    ) {
        assert!(chain_id >= 1 && chain_id <= 127, 1);
        let validators = validator_set::new(validators);
        let reference_gas_price = validator_set::derive_reference_gas_price(&validators);
        let state = SuiSystemState {
            // Use a hardcoded ID.
            id: object::sui_system_state(),
            chain_id,
            epoch: 0,
            validators,
            sui_supply,
            storage_fund,
            parameters: SystemParameters {
                min_validator_stake,
                max_validator_candidate_count,
                storage_gas_price
            },
            reference_gas_price,
            validator_report_records: vec_map::empty(),
            stake_subsidy: stake_subsidy::create(initial_stake_subsidy_amount),
        };
        transfer::share_object(state);
    }

    // ==== entry functions ====

    /// Can be called by anyone who wishes to become a validator in the next epoch.
    /// The `validator` object needs to be created before calling this.
    /// The amount of stake in the `validator` object must meet the requirements.
    // TODO: Does this need to go through a voting process? Any other criteria for
    // someone to become a validator?
    public entry fun request_add_validator(
        self: &mut SuiSystemState,
        pubkey_bytes: vector<u8>,
        network_pubkey_bytes: vector<u8>,
        proof_of_possession: vector<u8>,
        name: vector<u8>,
        net_address: vector<u8>,
        consensus_address: vector<u8>,
        worker_address: vector<u8>,
        stake: Coin<SUI>,
        gas_price: u64,
        commission_rate: u64,
        ctx: &mut TxContext,
    ) {
        assert!(
            validator_set::next_epoch_validator_count(&self.validators) < self.parameters.max_validator_candidate_count,
            ELIMIT_EXCEEDED,
        );
        let stake_amount = coin::value(&stake);
        assert!(
            stake_amount >= self.parameters.min_validator_stake,
            ELIMIT_EXCEEDED,
        );
        let validator = validator::new(
            tx_context::sender(ctx),
            pubkey_bytes,
            network_pubkey_bytes,
            proof_of_possession,
            name,
            net_address,
            consensus_address,
            worker_address,
            coin::into_balance(stake),
            option::none(),
            gas_price,
            commission_rate,
            ctx
        );

        validator_set::request_add_validator(&mut self.validators, validator);
    }

    /// A validator can call this function to request a removal in the next epoch.
    /// We use the sender of `ctx` to look up the validator
    /// (i.e. sender must match the sui_address in the validator).
    /// At the end of the epoch, the `validator` object will be returned to the sui_address
    /// of the validator.
    public entry fun request_remove_validator(
        self: &mut SuiSystemState,
        ctx: &mut TxContext,
    ) {
        validator_set::request_remove_validator(
            &mut self.validators,
            ctx,
        )
    }

    /// A validator can call this entry function to submit a new gas price quote, to be
    /// used for the reference gas price calculation at the end of the epoch.
    public entry fun request_set_gas_price(
        self: &mut SuiSystemState,
        new_gas_price: u64,
        ctx: &mut TxContext,
    ) {
        validator_set::request_set_gas_price(
            &mut self.validators,
            new_gas_price,
            ctx
        )
    }

    /// A validator can call this entry function to set a new commission rate, updated at the end of the epoch.
    public entry fun request_set_commission_rate(
        self: &mut SuiSystemState,
        new_commission_rate: u64,
        ctx: &mut TxContext,
    ) {
        validator_set::request_set_commission_rate(
            &mut self.validators,
            new_commission_rate,
            ctx
        )
    }

    /// A validator can request adding more stake. This will be processed at the end of epoch.
    public entry fun request_add_stake(
        self: &mut SuiSystemState,
        new_stake: Coin<SUI>,
        ctx: &mut TxContext,
    ) {
        validator_set::request_add_stake(
            &mut self.validators,
            coin::into_balance(new_stake),
            option::none(),
            ctx,
        )
    }

    /// A validator can request adding more stake using a locked coin. This will be processed at the end of epoch.
    public entry fun request_add_stake_with_locked_coin(
        self: &mut SuiSystemState,
        new_stake: LockedCoin<SUI>,
        ctx: &mut TxContext,
    ) {
        let (balance, epoch_time_lock) = locked_coin::into_balance(new_stake);
        validator_set::request_add_stake(
            &mut self.validators,
            balance,
            option::some(epoch_time_lock),
            ctx,
        )
    }

    /// A validator can request to withdraw stake.
    /// If the sender represents a pending validator (i.e. has just requested to become a validator
    /// in the current epoch and hence is not active yet), the stake will be withdrawn immediately
    /// and a coin with the withdraw amount will be sent to the validator's address.
    /// If the sender represents an active validator, the request will be processed at the end of epoch.
    public entry fun request_withdraw_stake(
        self: &mut SuiSystemState,
        stake: &mut Stake,
        withdraw_amount: u64,
        ctx: &mut TxContext,
    ) {
        validator_set::request_withdraw_stake(
            &mut self.validators,
            stake,
            withdraw_amount,
            self.parameters.min_validator_stake,
            ctx,
        )
    }

    /// Add delegated stake to a validator's staking pool.
    public entry fun request_add_delegation(
        self: &mut SuiSystemState,
        delegate_stake: Coin<SUI>,
        validator_address: address,
        ctx: &mut TxContext,
    ) {
        validator_set::request_add_delegation(
            &mut self.validators, 
            validator_address, 
            coin::into_balance(delegate_stake),
            option::none(),
            ctx,
        );
    }

    /// Add delegated stake to a validator's staking pool using a locked SUI coin.
    public entry fun request_add_delegation_with_locked_coin(
        self: &mut SuiSystemState,
        delegate_stake: LockedCoin<SUI>,
        validator_address: address,
        ctx: &mut TxContext,
    ) {
        let (balance, lock) = locked_coin::into_balance(delegate_stake);
        validator_set::request_add_delegation(&mut self.validators, validator_address, balance, option::some(lock), ctx);
    }

    /// Withdraw some portion of a delegation from a validator's staking pool.
    public entry fun request_withdraw_delegation(
        self: &mut SuiSystemState,
        delegation: &mut Delegation,
        staked_sui: &mut StakedSui,
        principal_withdraw_amount: u64,
        ctx: &mut TxContext,
    ) {
        validator_set::request_withdraw_delegation(
            &mut self.validators,
            delegation,
            staked_sui,
            principal_withdraw_amount,
            ctx,
        );
    }

    // Switch delegation from the current validator to a new one.
    public entry fun request_switch_delegation(
        self: &mut SuiSystemState,
        delegation: &mut Delegation,
        staked_sui: &mut StakedSui,
        new_validator_address: address,
        switch_pool_token_amount: u64,
        ctx: &mut TxContext,
    ) {
        validator_set::request_switch_delegation(
            &mut self.validators, delegation, staked_sui, new_validator_address, switch_pool_token_amount, ctx
        );
    }

    /// Report a validator as a bad or non-performant actor in the system.
    /// Suceeds iff both the sender and the input `validator_addr` are active validators
    /// and they are not the same address. This function is idempotent within an epoch.
    public entry fun report_validator(
        self: &mut SuiSystemState,
        validator_addr: address,
        ctx: &mut TxContext,
    ) {
        let sender = tx_context::sender(ctx);
        // Both the reporter and the reported have to be validators.
        assert!(validator_set::is_active_validator(&self.validators, sender), ENOT_VALIDATOR);
        assert!(validator_set::is_active_validator(&self.validators, validator_addr), ENOT_VALIDATOR);
        assert!(sender != validator_addr, ECANNOT_REPORT_ONESELF); 

        if (!vec_map::contains(&self.validator_report_records, &validator_addr)) {
            vec_map::insert(&mut self.validator_report_records, validator_addr, vec_set::singleton(sender));
        } else {
            let reporters = vec_map::get_mut(&mut self.validator_report_records, &validator_addr);
            if (!vec_set::contains(reporters, &sender)) {
                vec_set::insert(reporters, sender);
            }
        }
    }

    /// Undo a `report_validator` action. Aborts if the sender has not reported the
    /// `validator_addr` within this epoch.
    public entry fun undo_report_validator(
        self: &mut SuiSystemState,
        validator_addr: address,
        ctx: &mut TxContext,
    ) {
        let sender = tx_context::sender(ctx);

        assert!(vec_map::contains(&self.validator_report_records, &validator_addr), EREPORT_RECORD_NOT_FOUND);
        let reporters = vec_map::get_mut(&mut self.validator_report_records, &validator_addr);
        assert!(vec_set::contains(reporters, &sender), EREPORT_RECORD_NOT_FOUND);
        vec_set::remove(reporters, &sender);
    }

    /// This function should be called at the end of an epoch, and advances the system to the next epoch.
    /// It does the following things:
    /// 1. Add storage charge to the storage fund.
    /// 2. Burn the storage rebates from the storage fund. These are already refunded to transaction sender's
    ///    gas coins. 
    /// 3. Distribute computation charge to validator stake and delegation stake.
    /// 4. Update all validators.
    public entry fun advance_epoch(
        self: &mut SuiSystemState,
        new_epoch: u64,
        storage_charge: u64,
        computation_charge: u64,
        storage_rebate: u64,
        storage_fund_reinvest_rate: u64, // share of storage fund's rewards that's reinvested 
                                         // into storage fund, in basis point.
        ctx: &mut TxContext,
    ) {
        // Validator will make a special system call with sender set as 0x0.
        assert!(tx_context::sender(ctx) == @0x0, 0);

        let storage_reward = balance::create_staking_rewards(storage_charge);
        let computation_reward = balance::create_staking_rewards(computation_charge);

        // Include stake subsidy in the rewards given out to validators and delegators.
        stake_subsidy::advance_epoch(&mut self.stake_subsidy, &mut self.sui_supply);
        balance::join(&mut computation_reward, stake_subsidy::withdraw_all(&mut self.stake_subsidy));

        let delegation_stake = validator_set::total_delegation_stake(&self.validators);
        let validator_stake = validator_set::total_validator_stake(&self.validators);
        let storage_fund_balance = balance::value(&self.storage_fund);
        let total_stake = delegation_stake + validator_stake + storage_fund_balance;
        let total_stake_u128 = (total_stake as u128);
        let computation_charge_u128 = (computation_charge as u128);

        let delegator_reward_amount = (delegation_stake as u128) * computation_charge_u128 / total_stake_u128;
        let delegator_reward = balance::split(&mut computation_reward, (delegator_reward_amount as u64));
        balance::join(&mut self.storage_fund, storage_reward);

        let storage_fund_reward_amount = (storage_fund_balance as u128) * computation_charge_u128 / total_stake_u128;
        let storage_fund_reward = balance::split(&mut computation_reward, (storage_fund_reward_amount as u64));
        let storage_fund_reinvestment_amount = 
            storage_fund_reward_amount * (storage_fund_reinvest_rate as u128) / BASIS_POINT_DENOMINATOR;
        let storage_fund_reinvestment = balance::split(
            &mut storage_fund_reward,
            (storage_fund_reinvestment_amount as u64),
        );
        balance::join(&mut self.storage_fund, storage_fund_reinvestment);

        self.epoch = self.epoch + 1;
        // Sanity check to make sure we are advancing to the right epoch.
        assert!(new_epoch == self.epoch, 0);
        validator_set::advance_epoch(
            &mut self.validators,
            &mut computation_reward,
            &mut delegator_reward,
            &mut storage_fund_reward,
            &self.validator_report_records,
            ctx,
        );
        // Derive the reference gas price for the new epoch
        self.reference_gas_price = validator_set::derive_reference_gas_price(&self.validators);
        // Because of precision issues with integer divisions, we expect that there will be some
        // remaining balance in `delegator_reward`, `storage_fund_reward` and `computation_reward`. 
        // All of these go to the storage fund.
        balance::join(&mut self.storage_fund, delegator_reward);
        balance::join(&mut self.storage_fund, storage_fund_reward);
        balance::join(&mut self.storage_fund, computation_reward);

        // Destroy the storage rebate.
        assert!(balance::value(&self.storage_fund) >= storage_rebate, 0);
        balance::destroy_storage_rebates(balance::split(&mut self.storage_fund, storage_rebate));

        // Validator reports are only valid for the epoch.
        // TODO: or do we want to make it persistent and validators have to explicitly change their scores?
        self.validator_report_records = vec_map::empty();
    }

    spec advance_epoch {
        /// Total supply of SUI shouldn't change.
        ensures balance::supply_value(self.sui_supply) 
            == old(balance::supply_value(self.sui_supply));
    }

    /// Return the current epoch number. Useful for applications that need a coarse-grained concept of time,
    /// since epochs are ever-increasing and epoch changes are intended to happen every 24 hours.
    public fun epoch(self: &SuiSystemState): u64 {
        self.epoch
    }

    /// Returns the amount of stake delegated to `validator_addr`.
    /// Aborts if `validator_addr` is not an active validator.
    public fun validator_delegate_amount(self: &SuiSystemState, validator_addr: address): u64 {
        validator_set::validator_delegate_amount(&self.validators, validator_addr)
    }

    /// Returns the amount of stake `validator_addr` has.
    /// Aborts if `validator_addr` is not an active validator.
    public fun validator_stake_amount(self: &SuiSystemState, validator_addr: address): u64 {
        validator_set::validator_stake_amount(&self.validators, validator_addr)
    }

    /// Returns all the validators who have reported `addr` this epoch.
    public fun get_reporters_of(self: &SuiSystemState, addr: address): VecSet<address> {
        if (vec_map::contains(&self.validator_report_records, &addr)) {
            *vec_map::get(&self.validator_report_records, &addr)
        } else {
            vec_set::empty()
        }
    }

    #[test_only]
    public fun set_epoch_for_testing(self: &mut SuiSystemState, epoch_num: u64) {
        self.epoch = epoch_num
    }
}
