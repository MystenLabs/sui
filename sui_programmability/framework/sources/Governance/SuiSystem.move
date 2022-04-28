// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module Sui::SuiSystem {
    use Sui::Coin::{Self, Coin, TreasuryCap};
    use Sui::ID::VersionedID;
    use Sui::SUI::SUI;
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};
    use Sui::Validator::{Self, Validator};
    use Sui::ValidatorSet::{Self, ValidatorSet};

    friend Sui::Genesis;

    /// A list of system config parameters.
    // TDOO: We will likely add more, a few potential ones:
    // - the change in stake across epochs can be at most +/- x%
    // - the change in the validator set across epochs can be at most x validators
    struct SystemParameters has store {
        /// Lower-bound on the amount of stake required to become a validator.
        min_validator_stake: u64,
        /// Upper-bound on the amount of stake allowed to become a validator.
        max_validator_stake: u64,
        /// Maximum number of validator candidates at any moment.
        /// We do not allow the number of validators in any epoch to go above this.
        max_validator_candidate_count: u64,
    }

    /// The top-level object containing all information of the Sui system.
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

    // ==== functions that can only be called by Genesis ====

    /// Create a new SuiSystemState object and make it shared.
    /// This function will be called only once in Genesis.
    public(friend) fun create(
        validators: vector<Validator>,
        treasury_cap: TreasuryCap<SUI>,
        storage_fund: Coin<SUI>,
        max_validator_candidate_count: u64,
        min_validator_stake: u64,
        max_validator_stake: u64,
        ctx: &mut TxContext,
    ) {
        assert!(min_validator_stake < max_validator_stake, 0);
        let state = SuiSystemState {
            id: TxContext::new_id(ctx),
            epoch: 0,
            validators: ValidatorSet::new(validators),
            treasury_cap,
            storage_fund,
            parameters: SystemParameters {
                min_validator_stake,
                max_validator_stake,
                max_validator_candidate_count,
            },
        };
        Transfer::share_object(state);
    }

    // ==== entry functions ====

    /// Can be called by anyone who wishes to become a validator in the next epoch.
    /// The `validator` object needs to be created before calling this.
    /// The amount of stake in the `validator` object must meet the requirements.
    // TODO: Does this need to go through a voting process? Any other criteria for
    // someone to become a validator?
    public(script) fun request_add_validator(
        self: &mut SuiSystemState,
        name: vector<u8>,
        net_address: vector<u8>,
        stake: Coin<SUI>,
        ctx: &mut TxContext,
    ) {
        assert!(
            ValidatorSet::get_total_validator_candidate_count(&self.validators) < self.parameters.max_validator_candidate_count,
            0
        );
        let stake_amount = Coin::value(&stake);
        assert!(
            stake_amount >= self.parameters.min_validator_stake
                && stake_amount <= self.parameters.max_validator_stake,
            0
        );
        let validator = Validator::new(TxContext::sender(ctx), name, net_address, stake);
        ValidatorSet::request_add_validator(&mut self.validators, validator);
    }

    /// A validator can call this function to request a removal in the next epoch.
    /// We use the sender of `ctx` to look up the validator
    /// (i.e. sender must match the sui_address in the validator).
    /// At the end of the epoch, the `validator` object will be returned to the sui_address
    /// of the validator.
    public(script) fun request_remove_validator(
        self: &mut SuiSystemState,
        ctx: &mut TxContext,
    ) {
        ValidatorSet::request_remove_validator(
            &mut self.validators,
            ctx,
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
            self.parameters.max_validator_stake,
            ctx,
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
            self.parameters.min_validator_stake,
            ctx,
        )
    }

    /// This function should be called at the end of an epoch, and advances the system to the next epoch.
    public(script) fun advance_epoch(
        self: &mut SuiSystemState,
        new_epoch: u64,
        ctx: &mut TxContext,
    ) {
        // Only an active validator can make a call to this function.
        assert!(ValidatorSet::is_active_validator(&self.validators, TxContext::sender(ctx)), 0);

        self.epoch = self.epoch + 1;
        // Sanity check to make sure we are advancing to the right epoch.
        assert!(new_epoch == self.epoch, 0);
        ValidatorSet::advance_epoch(
            &mut self.validators,
            ctx,
        )
    }
}
