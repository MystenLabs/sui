// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module Sui::SuiSystem {
    use Sui::Coin::{Coin, TreasuryCap};
    use Sui::ID::VersionedID;
    use Sui::SUI::SUI;
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};
    use Sui::Validator::{Self, Validator};
    use Sui::ValidatorSet::{Self, ValidatorSet};

    friend Sui::Genesis;

    /// A wrong epoch ID is passed in when trying to advance epoch.
    /// This shouldn't happen unless there is a bug in the validator code.
    const EINVALID_EPOCH: u64 = 0;

    /// This happens when a new validator request is sent, but the new validator's
    /// stake amount does not meet the requirement.
    const EINVALID_STAKE_AMOUNT: u64 = 1;

    /// A list of config parameters that may change from epoch to epoch.
    struct SystemParameters has store {
        /// Lower-bound on the amount of stake required to become a validator.
        min_validator_stake: u64,

        /// Upper-bound on the amount of stake allowed to become a validator.
        max_validator_stake: u64,
    }

    /// The to-level object containing all information of the Sui system.
    struct SuiSystemState has key {
        id: VersionedID,
        /// The current epoch ID, starting from 0.
        epoch: u64,
        /// Contains all information about the validators.
        validators: ValidatorSet,
        /// The SUI treasury capability needed to mint SUI.
        treasury_cap: TreasuryCap<SUI>,
        /// The storage fund.
        storage_fund: Coin<SUI>,
        /// A list of system config parameters.
        parameters: SystemParameters,
    }

    /// Create a new SuiSystemState object and make it shared.
    /// This function will be called only once in Genesis.
    public(friend) fun create(
        validators: vector<Validator>,
        treasury_cap: TreasuryCap<SUI>,
        storage_fund: Coin<SUI>,
        min_validator_stake: u64,
        max_validator_stake: u64,
        ctx: &mut TxContext,
    ) {
        let state = SuiSystemState {
            id: TxContext::new_id(ctx),
            epoch: 0,
            validators: ValidatorSet::new(validators),
            treasury_cap,
            storage_fund,
            parameters: SystemParameters {
                min_validator_stake,
                max_validator_stake,
            },
        };
        Transfer::share_object(state);
    }

    /// Can be called by anyone who wishes to become a validator in the next epoch.
    /// At the end of the current epoch, the system will look at the amount of stake
    /// compared to other validator candidates to decide if it is eligible.
    /// If not, the `validator` object will be returned to `validator.sui_address` at that time.
    ///
    /// The `validator` object needs to be created before calling this.
    /// The amount of stake in the `validator` object must meet the requirements.
    public(script) fun request_add_validator(
        self: &mut SuiSystemState,
        validator: Validator,
        _ctx: &mut TxContext,
    ) {
        let stake_amount = Validator::get_stake_amount(&validator);
        assert!(
            stake_amount >= self.parameters.min_validator_stake
                && stake_amount <= self.parameters.max_validator_stake,
            EINVALID_STAKE_AMOUNT
        );
        ValidatorSet::request_add_validator(&mut self.validators, validator);
    }

    /// A validator can call this function to request a withdraw in the next epoch.
    /// We use the sender of `ctx` to look up the validator
    /// (i.e. sender must match the sui_address in the validator).
    /// At the end of the epoch, the `validator` object will be returned to the sui_address
    /// of the validator.
    public(script) fun request_withdraw_validator(
        self: &mut SuiSystemState,
        ctx: &mut TxContext,
    ) {
        ValidatorSet::request_withdraw_validator(
            &mut self.validators,
            TxContext::sender(ctx),
        )
    }

    /// A validator can request adding more stake. This will be processed at the end of epoch.
    public(script) fun request_add_stake(
        self: &mut SuiSystemState,
        new_stake: Coin<SUI>,
        ctx: &mut TxContext,
    ) {
        ValidatorSet::request_add_stake(
            &mut self.validators,
            new_stake,
            TxContext::sender(ctx),
        )
    }

    /// A validator can request to withdraw stake.
    /// If the sender represents a pending validator (i.e. has just requested to become a validator
    /// in the current epoch and hence is not active yet), the stake will be withdrawn immediately
    /// and a coin with the withdraw amount will be sent to the validator's address.
    /// If the sender represents an active validator, the request will be processed at the end of epoch.
    public(script) fun request_withdraw_stake(
        self: &mut SuiSystemState,
        withdraw_amount: u64,
        ctx: &mut TxContext,
    ) {
        ValidatorSet::request_withdraw_stake(
            &mut self.validators,
            withdraw_amount,
            ctx,
        )
    }

    /// This function should be called at the end of an epoch, and advances the system to the next epoch.
    /// All system parameters can be changed at this time.
    /// The number of validators can also be adjusted through `new_active_validator_count`.
    /// If however there are not enough validators, this function can fail.
    public(script) fun advance_epoch(
        self: &mut SuiSystemState,
        new_epoch: u64,
        new_active_validator_count: u64,
        new_min_validator_stake: u64,
        new_max_validator_stake: u64,
        ctx: &mut TxContext,
    ) {
        self.epoch = self.epoch + 1;
        // Sanity check to make sure we are advancing to the right epoch.
        assert!(new_epoch == self.epoch, EINVALID_EPOCH);
        ValidatorSet::advance_epoch(
            &mut self.validators,
            new_active_validator_count,
            new_min_validator_stake,
            new_max_validator_stake,
            ctx,
        );
        self.parameters.min_validator_stake = new_min_validator_stake;
        self.parameters.max_validator_stake = new_max_validator_stake;
    }
}