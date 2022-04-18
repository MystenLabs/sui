// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module Sui::ValidatorSet {
    use Std::Vector;

    use Sui::Coin::Coin;
    use Sui::SUI::SUI;
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};
    use Sui::Validator::{Self, Validator};

    friend Sui::SuiSystem;

    const EDUPLICATE_VALIDATOR: u64 = 0;

    const EINVALID_VALIDATOR_INDEX: u64 = 1;

    const EVALIDATOR_NOT_FOUND: u64 = 2;

    const EDUPLICATE_WITHDRAW: u64 = 3;

    const ENOT_ENOUGH_VALIDATORS: u64 = 4;

    struct ValidatorSet has store {
        /// The current list of active validators.
        /// The stake amount is sorted, with highest stake first.
        active_validators: vector<Validator>,

        /// List of new validator candidates added during the current epoch.
        /// They will be processed at the end of the epoch.
        pending_validators: vector<Validator>,

        /// Withdraw requests from the validators. Each element is an index
        /// pointing to `active_validators`.
        pending_withdraws: vector<u64>,
    }

    public(friend) fun new(init_active_validators: vector<Validator>): ValidatorSet {
        ValidatorSet {
            active_validators: init_active_validators,
            pending_validators: Vector::empty(),
            pending_withdraws: Vector::empty(),
        }
    }

    /// Called by `SuiSystem`, add a new validator to `pending_validators`, which will be
    /// processed at the end of epoch.
    public(friend) fun request_add_validator(self: &mut ValidatorSet, validator: Validator) {
        assert!(
            contains_duplicate_validator(&self.active_validators, &validator)
                && contains_duplicate_validator(&self.pending_validators, &validator),
            EDUPLICATE_VALIDATOR
        );
        Vector::push_back(&mut self.pending_validators, validator);
    }

    /// Called by `SuiSystem`, to withdraw a validator.
    /// If the validator to be withdrawn is a pending one, it's removed immediately and returned
    /// to the validator's sui_address; otherwise the index is added to `pending_withdraws` and
    /// will be processed at the end of epoch.
    public(friend) fun request_withdraw_validator(
        self: &mut ValidatorSet,
        validator_address: address,
    ) {
        let (found, validator_index) = find_validator(&self.pending_validators, validator_address);
        if (found) {
            let v = Vector::remove(&mut self.pending_validators, validator_index);
            Transfer::transfer(v, validator_address);
            return
        };
        let (found, validator_index) = find_validator(&self.active_validators, validator_address);
        assert!(found, EVALIDATOR_NOT_FOUND);
        assert!(
            !Vector::contains(&self.pending_withdraws, &validator_index),
            EDUPLICATE_WITHDRAW
        );
        Vector::push_back(&mut self.pending_withdraws, validator_index);
    }

    /// Called by `SuiSystem`, to add more stake to a validator.
    /// If the validator is a pending one, we add stake to it directly; otherwise we send a request
    /// to add the extra stake at the end of epoch.
    public(friend) fun request_add_stake(
        self: &mut ValidatorSet,
        new_stake: Coin<SUI>,
        validator_address: address,
    ) {
        let (found, validator_index) = find_validator(&self.pending_validators, validator_address);
        if (found) {
            let validator = Vector::borrow_mut(&mut self.pending_validators, validator_index);
            Validator::add_stake_to_pending_validator(validator, new_stake);
        } else {
            let (found, validator_index) = find_validator(&self.active_validators, validator_address);
            assert!(found, EVALIDATOR_NOT_FOUND);
            let validator = Vector::borrow_mut(&mut self.active_validators, validator_index);
            Validator::request_add_stake_to_active_validator(validator, new_stake);
        }
    }

    /// Called by `SuiSystem`, to withdraw stake from a validator.
    /// If the validator is a pending one, we withdraw from it and send back the coins immediately;
    /// otherwise we send a withdraw request which will be processed at the end of epoch.
    public(friend) fun request_withdraw_stake(
        self: &mut ValidatorSet,
        withdraw_amount: u64,
        ctx: &mut TxContext,
    ) {
        let validator_address = TxContext::sender(ctx);
        let (found, validator_index) = find_validator(&self.pending_validators, validator_address);
        if (found) {
            let validator = Vector::borrow_mut(&mut self.pending_validators, validator_index);
            Validator::withdraw_stake_from_pending_validator(validator, withdraw_amount, ctx);
        } else {
            let (found, validator_index) = find_validator(&self.active_validators, validator_address);
            assert!(found, EVALIDATOR_NOT_FOUND);
            let validator = Vector::borrow_mut(&mut self.active_validators, validator_index);
            Validator::request_withdraw_stake_from_active_validator(validator, withdraw_amount);
        }
    }

    /// Update the validator set at the end of epoch.
    /// It does the following things:
    ///   1. Distribute stake award.
    ///   2. Process pending stake deposits and withdraws for each validator (`adjust_stake`).
    ///   3. Process pending validator application and withdraws.
    ///   4. At the end, we decide which validators will form the validator set for the next epoch.
    public(friend) fun advance_epoch(
        self: &mut ValidatorSet,
        new_validator_set_size: u64,
        new_min_validator_stake: u64,
        new_max_validator_stake: u64,
        ctx: &mut TxContext,
    ) {
        // TODO: Distribute stake rewards.

        adjust_stake(&mut self.active_validators, ctx);

        process_pending_withdraws(&mut self.active_validators, &mut self.pending_withdraws);
        process_pending_validators(&mut self.active_validators, &mut self.pending_validators);

        decide_new_active_validators(
            &mut self.active_validators,
            new_validator_set_size,
            new_min_validator_stake,
            new_max_validator_stake,
        );
    }

    /// Checks whether a duplicate of `new_validator` is already in `validators`.
    /// Two validators duplicate if they share the same sui_address or same IP or same name.
    fun contains_duplicate_validator(validators: &vector<Validator>, new_validator: &Validator): bool {
        let len = Vector::length(validators);
        let i = 0;
        while (i < len) {
            let v = Vector::borrow(validators, i);
            if (Validator::duplicates_with(v, new_validator)) {
                return true
            };
            i = i + 1;
        };
        false
    }

    /// Find validator by `validator_address`, in `validators`.
    /// Returns (true, index) if the validator is found, and the index is its index in the list.
    /// If not found, returns (false, 0).
    fun find_validator(validators: &vector<Validator>, validator_address: address): (bool, u64) {
        let length = Vector::length(validators);
        let i = 0;
        while (i < length) {
            let v = Vector::borrow(validators, i);
            if (Validator::get_sui_address(v) == validator_address) {
                return (true, i)
            };
            i = i + 1;
        };
        (false, 0)
    }

    /// Process the pending withdraw requests. For each pending request, the validator
    /// is removed from `validators` and sent back to the address of the validator.
    fun process_pending_withdraws(validators: &mut vector<Validator>, withdraw_list: &mut vector<u64>) {
        sort_withdraw_list(withdraw_list);
        while (!Vector::is_empty(withdraw_list)) {
            let index = Vector::pop_back(withdraw_list);
            let validator = Vector::remove(validators, index);
            Validator::send_back(validator);
        }
    }

    /// Process the pending new validators. They are simply inserted into `validators`.
    fun process_pending_validators(validators: &mut vector<Validator>, pending_validators: &mut vector<Validator>) {
        while (!Vector::is_empty(pending_validators)) {
            let v = Vector::pop_back(pending_validators);
            Vector::push_back(validators, v);
        }
    }

    /// Given a list of all `validators`, each with stake updated properly, decide
    /// who could become a validator in the next epoch.
    /// A validator is chosen if its stake is in the range of min/max stake specified,
    /// and is among the top `new_validator_set_size` in terms of stake amount.
    fun decide_new_active_validators(
        validators: &mut vector<Validator>,
        new_validator_set_size: u64,
        new_min_validator_stake: u64,
        new_max_validator_stake: u64,
    ) {
        sort_validators_by_stake(validators);
        let removed_validators = Vector::empty();
        while (!Vector::is_empty(validators)) {
            let v = Vector::borrow(validators, 0);
            if (Validator::get_stake_amount(v) > new_max_validator_stake) {
                Vector::push_back(&mut removed_validators, Vector::remove(validators, 0));
            } else {
                break
            }
        };
        while (!Vector::is_empty(validators)) {
            let length = Vector::length(validators);
            let v = Vector::borrow(validators, length - 1);
            if (Validator::get_stake_amount(v) < new_min_validator_stake) {
                Vector::push_back(&mut removed_validators, Vector::pop_back(validators));
            } else {
                break
            }
        };
        let length = Vector::length(validators);
        // Note: This may cause epoch advancement to fail, if we don't
        // have enough validators for the next epoch.
        assert!(
            length >= new_validator_set_size,
            ENOT_ENOUGH_VALIDATORS
        );
        while (!Vector::is_empty(&removed_validators)) {
            let v = Vector::pop_back(&mut removed_validators);
            Validator::send_back(v)
        };
        Vector::destroy_empty(removed_validators)
    }

    /// Sort all the pending withdraw indexes.
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

    /// Sort `validators` by their stake amount, largest first.
    /// The list should already be almost sorted. This is because we sort them
    /// at the end of each epoch. Newly added ones as well as stake adjustments
    /// can change order, but not significantly.
    /// For the above reason, we use insertion sort as it's most efficient.
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

    /// Process the pending stake changes for each validator.
    fun adjust_stake(validators: &mut vector<Validator>, ctx: &mut TxContext) {
        let length = Vector::length(validators);
        let i = 0;
        while (i < length) {
            let validator = Vector::borrow_mut(validators, i);
            Validator::adjust_stake(validator, ctx);
            i = i + 1;
        }
    }
}