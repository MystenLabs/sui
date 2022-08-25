// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::sui_system {
    use sui::balance::{Self, Balance, Supply};
    use sui::coin::{Self, Coin};
    use sui::delegation::{Self, Delegation};
    use sui::epoch_reward_record::{Self, EpochRewardRecord};
    use sui::object::{Self, UID};
    use sui::locked_coin::{Self, LockedCoin};
    use sui::sui::SUI;
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use sui::validator::{Self, Validator};
    use sui::validator_set::{Self, ValidatorSet};
    use sui::stake::Stake;
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
        /// The delegation reward pool. All delegation reward goes into this.
        /// Delegation reward claims withdraw from this.
        delegation_reward: Balance<SUI>,
        /// The reference gas price for the current epoch.
        reference_gas_price: u64,
    }

    // ==== functions that can only be called by Genesis ====

    /// Create a new SuiSystemState object and make it shared.
    /// This function will be called only once in Genesis.
    public(friend) fun create(
        validators: vector<Validator>,
        sui_supply: Supply<SUI>,
        storage_fund: Balance<SUI>,
        max_validator_candidate_count: u64,
        min_validator_stake: u64,
        storage_gas_price: u64,
    ) {
        let validators = validator_set::new(validators);
        let reference_gas_price = validator_set::derive_reference_gas_price(&validators);
        let state = SuiSystemState {
            // Use a hardcoded ID.
            id: object::sui_system_state(),
            epoch: 0,
            validators,
            sui_supply,
            storage_fund,
            parameters: SystemParameters {
                min_validator_stake,
                max_validator_candidate_count,
                storage_gas_price
            },
            delegation_reward: balance::zero(),
            reference_gas_price,
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
        stake: Coin<SUI>,
        gas_price: u64,
        ctx: &mut TxContext,
    ) {
        assert!(
            validator_set::next_epoch_validator_count(&self.validators) < self.parameters.max_validator_candidate_count,
            0
        );
        let stake_amount = coin::value(&stake);
        assert!(
            stake_amount >= self.parameters.min_validator_stake,
            0
        );
        let validator = validator::new(
            tx_context::sender(ctx),
            pubkey_bytes,
            network_pubkey_bytes,
            proof_of_possession,
            name,
            net_address,
            coin::into_balance(stake),
            option::none(),
            gas_price,
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

    public entry fun request_add_delegation(
        self: &mut SuiSystemState,
        delegate_stake: Coin<SUI>,
        validator_address: address,
        ctx: &mut TxContext,
    ) {
        let amount = coin::value(&delegate_stake);
        validator_set::request_add_delegation(&mut self.validators, validator_address, amount);

        // Delegation starts from the next epoch.
        let starting_epoch = self.epoch + 1;
        delegation::create(starting_epoch, validator_address, delegate_stake, ctx);
    }

    public entry fun request_add_delegation_with_locked_coin(
        self: &mut SuiSystemState,
        delegate_stake: LockedCoin<SUI>,
        validator_address: address,
        ctx: &mut TxContext,
    ) {
        let amount = locked_coin::value(&delegate_stake);
        validator_set::request_add_delegation(&mut self.validators, validator_address, amount);

        // Delegation starts from the next epoch.
        let starting_epoch = self.epoch + 1;
        delegation::create_from_locked_coin(starting_epoch, validator_address, delegate_stake, ctx);
    }

    public entry fun request_remove_delegation(
        self: &mut SuiSystemState,
        delegation: &mut Delegation,
        ctx: &mut TxContext,
    ) {
        validator_set::request_remove_delegation(
            &mut self.validators,
            delegation::validator(delegation),
            delegation::delegate_amount(delegation),
        );
        delegation::undelegate(delegation, self.epoch, ctx)
    }

    // Switch delegation from the current validator to a new one.
    public entry fun request_switch_delegation(
        self: &mut SuiSystemState,
        delegation: &mut Delegation,
        new_validator_address: address,
        ctx: &mut TxContext,
    ) {
        let old_validator_address = delegation::validator(delegation);
        let amount = delegation::delegate_amount(delegation);
        validator_set::request_remove_delegation(&mut self.validators, old_validator_address, amount);
        validator_set::request_add_delegation(&mut self.validators, new_validator_address, amount);
        delegation::switch_delegation(delegation, new_validator_address, ctx);
    }

    // TODO: Once we support passing vector of object references as arguments,
    // we should support passing a vector of &mut EpochRewardRecord,
    // which will allow delegators to claim all their reward in one transaction.
    public entry fun claim_delegation_reward(
        self: &mut SuiSystemState,
        delegation: &mut Delegation,
        epoch_reward_record: &mut EpochRewardRecord,
        ctx: &mut TxContext,
    ) {
        let epoch = epoch_reward_record::epoch(epoch_reward_record);
        let validator = epoch_reward_record::validator(epoch_reward_record);
        assert!(delegation::can_claim_reward(delegation, epoch, validator), 0);
        let reward_amount = epoch_reward_record::claim_reward(
            epoch_reward_record,
            delegation::delegate_amount(delegation),
        );
        let reward = balance::split(&mut self.delegation_reward, reward_amount);
        delegation::claim_reward(delegation, reward, ctx);
    }

    /// This function should be called at the end of an epoch, and advances the system to the next epoch.
    /// It does the following things:
    /// 1. Add storage charge to the storage fund.
    /// 2. Distribute computation charge to validator stake and delegation stake.
    /// 3. Create reward information records for each validator in this epoch.
    /// 4. Update all validators.
    public entry fun advance_epoch(
        self: &mut SuiSystemState,
        new_epoch: u64,
        storage_charge: u64,
        computation_charge: u64,
        ctx: &mut TxContext,
    ) {
        // Validator will make a special system call with sender set as 0x0.
        assert!(tx_context::sender(ctx) == @0x0, 0);

        let storage_reward = balance::increase_supply(&mut self.sui_supply, storage_charge);
        let computation_reward = balance::increase_supply(&mut self.sui_supply, computation_charge);

        let delegation_stake = validator_set::total_delegation_stake(&self.validators);
        let validator_stake = validator_set::total_validator_stake(&self.validators);
        let storage_fund = balance::value(&self.storage_fund);
        let total_stake = delegation_stake + validator_stake + storage_fund;

        let delegator_reward_amount = delegation_stake * computation_charge / total_stake;
        let delegator_reward = balance::split(&mut computation_reward, delegator_reward_amount);
        balance::join(&mut self.storage_fund, storage_reward);
        balance::join(&mut self.delegation_reward, delegator_reward);

        validator_set::create_epoch_records(
            &self.validators,
            self.epoch,
            computation_charge,
            total_stake,
            ctx,
        );

        self.epoch = self.epoch + 1;
        // Sanity check to make sure we are advancing to the right epoch.
        assert!(new_epoch == self.epoch, 0);
        validator_set::advance_epoch(
            &mut self.validators,
            &mut computation_reward,
            ctx,
        );
        // Derive the reference gas price for the new epoch
        self.reference_gas_price = validator_set::derive_reference_gas_price(&self.validators);
        // Because of precision issues with integer divisions, we expect that there will be some
        // remaining balance in `computation_reward`. All of these go to the storage fund.
        balance::join(&mut self.storage_fund, computation_reward);
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

    /// Returns the amount of delegators who have delegated to `validator_addr`.
    /// Aborts if `validator_addr` is not an active validator.
    public fun validator_delegator_count(self: &SuiSystemState, validator_addr: address): u64 {
        validator_set::validator_delegator_count(&self.validators, validator_addr)
    }

    #[test_only]
    public fun set_epoch_for_testing(self: &mut SuiSystemState, epoch_num: u64) {
        self.epoch = epoch_num
    }
}
