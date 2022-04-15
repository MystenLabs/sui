// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module Sui::ValidatorSet {
    use Std::Vector;
    use Sui::Validator::{Self, Validator};

    friend Sui::SuiSystem;

    const EDUPLICATE_VALIDATOR: u64 = 0;

    const EINVALID_VALIDATOR_INDEX: u64 = 1;

    const EVALIDATOR_NOT_FOUND: u64 = 2;

    const EDUPLICATE_WITHDRAW: u64 = 3;

    const ENOT_ENOUGH_VALIDATORS: u64 = 4;

    struct ValidatorSet has store {
        validator_map: vector<Validator>,
        active_validator_count: u64,
        withdraw_list: vector<u64>,
    }

    public(friend) fun new(init_active_validators: vector<Validator>): ValidatorSet {
        let active_validator_count = Vector::length(&init_active_validators);
        ValidatorSet {
            validator_map: init_active_validators,
            active_validator_count,
            withdraw_list: vector[],
        }
    }

    public(friend) fun request_add_validator(self: &mut ValidatorSet, validator: Validator) {
        let validators = &mut self.validator_map;
        let len = Vector::length(validators);
        let i = 0;
        while (i < len) {
            let v = Vector::borrow(validators, i);
            assert!(
                !Validator::duplicates_with(v, &validator),
                EDUPLICATE_VALIDATOR
            );
            i = i + 1;
        };
        Vector::push_back(validators, validator);
    }

    public(friend) fun request_withdraw_validator(
        self: &mut ValidatorSet,
        validator_address: address,
    ) {
        let validator_index = find_validator(&self.validator_map, validator_address);
        assert!(
            !Vector::contains(&self.withdraw_list, &validator_index),
            EDUPLICATE_WITHDRAW
        );
        Vector::push_back(&mut self.withdraw_list, validator_index);
    }

    public(friend) fun advance_epoch(
        self: &mut ValidatorSet,
        new_active_validator_count: u64,
    ) {
        remove_validators(&mut self.validator_map, &mut self.withdraw_list);
        sort_validators_by_stake(&mut self.validator_map);
        assert!(
            Vector::length(&self.validator_map) >= new_active_validator_count,
            ENOT_ENOUGH_VALIDATORS
        );
        self.active_validator_count = new_active_validator_count;
    }

    public fun get_active_validator_count(self: &ValidatorSet): u64 {
        self.active_validator_count
    }

    fun find_validator(validators: &vector<Validator>, validator_address: address): u64 {
        let length = Vector::length(validators);
        let i = 0;
        while (i < length) {
            let v = Vector::borrow(validators, i);
            if (Validator::get_sui_address(v) == validator_address) {
                return i
            };
            i = i + 1;
        };
        abort EVALIDATOR_NOT_FOUND
    }

    fun remove_validators(validators: &mut vector<Validator>, withdraw_list: &mut vector<u64>) {
        sort_withdraw_list(withdraw_list);
        while (!Vector::is_empty(withdraw_list)) {
            let index = Vector::pop_back(withdraw_list);
            let validator = Vector::remove(validators, index);
            Validator::send_back(validator);
        }
    }

    fun sort_withdraw_list(withdraw_list: &mut vector<u64>) {
        let length = Vector::length(withdraw_list);
        let i = 1;
        while (i < length) {
            let cur = *Vector::borrow(withdraw_list, i);
            let j = i;
            while (j > 0) {
                j = j - 1;
                if (*Vector::borrow(withdraw_list, j) > cur) {
                    Vector::swap(withdraw_list, j, j + 1);
                } else {
                    break
                };
            };
            i = i + 1;
        };
    }

    fun sort_validators_by_stake(validators: &mut vector<Validator>) {
        let length = Vector::length(validators);
        let i = 1;
        while (i < length) {
            let next = Validator::get_stake_amount(Vector::borrow(validators, i));
            let j = i;
            while (j > 0) {
                j = j - 1;
                let prev: u64 = Validator::get_stake_amount(Vector::borrow(validators, j));
                if (prev < next) {
                    Vector::swap(validators, j, j + 1);
                } else {
                    break
                };
            };
            i = i + 1;
        };
    }
}