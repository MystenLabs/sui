// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module Sui::SuiSystem {
    use Sui::ID::VersionedID;
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};
    use Sui::Validator::{Self, Validator};
    use Sui::ValidatorSet::{Self, ValidatorSet};

    friend Sui::Genesis;

    const MINIMUM_VALIDATOR_STAKE: u64 = 10000;

    const EINVALID_EPOCH: u64 = 0;

    const EINSUFFICIENT_STAKE: u64 = 1;

    struct SuiSystemState has key {
        id: VersionedID,
        epoch: u64,
        validators: ValidatorSet,
    }

    public(friend) fun create(
        validators: vector<Validator>,
        ctx: &mut TxContext,
    ) {
        let state = SuiSystemState {
            id: TxContext::new_id(ctx),
            epoch: 0,
            validators: ValidatorSet::new(validators),
        };
        Transfer::share_object(state);
    }

    public(script) fun request_add_validator(
        self: &mut SuiSystemState,
        validator: Validator,
        _ctx: &mut TxContext,
    ) {
        assert!(
            Validator::get_stake_amount(&validator) >= MINIMUM_VALIDATOR_STAKE,
            EINSUFFICIENT_STAKE
        );
        ValidatorSet::request_add_validator(&mut self.validators, validator);
    }

    public(script) fun request_withdraw_validator(
        self: &mut SuiSystemState,
        ctx: &mut TxContext,
    ) {
        ValidatorSet::request_withdraw_validator(
            &mut self.validators,
            TxContext::sender(ctx),
        )
    }

    public(script) fun advance_epoch(
        self: &mut SuiSystemState,
        new_epoch: u64,
        new_active_validator_count: u64,
        _ctx: &mut TxContext,
    ) {
        assert!(new_epoch == self.epoch + 1, EINVALID_EPOCH);
        ValidatorSet::advance_epoch(&mut self.validators, new_active_validator_count);
    }
}