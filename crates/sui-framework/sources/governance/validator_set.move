// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::validator_set {
    use std::option::{Self, Option};
    use std::vector;

    use sui::balance::{Self, Balance};
    use sui::epoch_reward_record;
    use sui::sui::SUI;
    use sui::tx_context::{Self, TxContext};
    use sui::validator::{Self, Validator, ValidatorMetadata};

    friend sui::sui_system;

    #[test_only]
    friend sui::validator_set_tests;

    struct ValidatorSet has store {
        /// Total amount of stake from all active validators (not including delegation),
        /// at the beginning of the epoch.
        validator_stake: u64,

        /// Total amount of stake from delegation, at the beginning of the epoch.
        delegation_stake: u64,

        /// The amount of accumulated stake to reach a quorum among all active validators.
        /// This is always 2/3 of total stake. Keep it here to reduce potential inconsistencies
        /// among validators.
        quorum_stake_threshold: u64,

        /// The current list of active validators.
        active_validators: vector<Validator>,

        /// List of new validator candidates added during the current epoch.
        /// They will be processed at the end of the epoch.
        pending_validators: vector<Validator>,

        /// Removal requests from the validators. Each element is an index
        /// pointing to `active_validators`.
        pending_removals: vector<u64>,

        /// The metadata of the validator set for the next epoch. This is kept up-to-dated.
        /// Everytime a change request is received, this set is updated.
        next_epoch_validators: vector<ValidatorMetadata>,
    }

    public(friend) fun new(init_active_validators: vector<Validator>): ValidatorSet {
        let (validator_stake, delegation_stake, quorum_stake_threshold) = calculate_total_stake_and_quorum_threshold(&init_active_validators);
        let validators = ValidatorSet {
            validator_stake,
            delegation_stake,
            quorum_stake_threshold,
            active_validators: init_active_validators,
            pending_validators: vector::empty(),
            pending_removals: vector::empty(),
            next_epoch_validators: vector::empty(),
        };
        validators.next_epoch_validators = derive_next_epoch_validators(&validators);
        validators
    }

    /// Get the total number of candidates that might become validators in the next epoch.
    public(friend) fun total_validator_candidate_count(self: &ValidatorSet): u64 {
        vector::length(&self.active_validators)
            + vector::length(&self.pending_validators)
            - vector::length(&self.pending_removals)
    }

    /// Called by `SuiSystem`, add a new validator to `pending_validators`, which will be
    /// processed at the end of epoch.
    public(friend) fun request_add_validator(self: &mut ValidatorSet, validator: Validator) {
        assert!(
            !contains_duplicate_validator(&self.active_validators, &validator)
                && !contains_duplicate_validator(&self.pending_validators, &validator),
            0
        );
        vector::push_back(&mut self.pending_validators, validator);
        self.next_epoch_validators = derive_next_epoch_validators(self);
    }

    /// Called by `SuiSystem`, to remove a validator.
    /// The index of the validator is added to `pending_removals` and
    /// will be processed at the end of epoch.
    /// Only an active validator can request to be removed.
    public(friend) fun request_remove_validator(
        self: &mut ValidatorSet,
        ctx: &TxContext,
    ) {
        let validator_address = tx_context::sender(ctx);
        let validator_index_opt = find_validator(&self.active_validators, validator_address);
        assert!(option::is_some(&validator_index_opt), 0);
        let validator_index = option::extract(&mut validator_index_opt);
        assert!(
            !vector::contains(&self.pending_removals, &validator_index),
            0
        );
        vector::push_back(&mut self.pending_removals, validator_index);
        self.next_epoch_validators = derive_next_epoch_validators(self);
    }

    /// Called by `SuiSystem`, to add more stake to a validator.
    /// The new stake will be added to the validator's pending stake, which will be processed
    /// at the end of epoch.
    /// The total stake of the validator cannot exceed `max_validator_stake` with the `new_stake`.
    public(friend) fun request_add_stake(
        self: &mut ValidatorSet,
        new_stake: Balance<SUI>,
        ctx: &TxContext,
    ) {
        let validator_address = tx_context::sender(ctx);
        let validator = get_validator_mut(&mut self.active_validators, validator_address);
        validator::request_add_stake(validator, new_stake);
        self.next_epoch_validators = derive_next_epoch_validators(self);
    }

    /// Called by `SuiSystem`, to withdraw stake from a validator.
    /// We send a withdraw request to the validator which will be processed at the end of epoch.
    /// The remaining stake of the validator cannot be lower than `min_validator_stake`.
    public(friend) fun request_withdraw_stake(
        self: &mut ValidatorSet,
        withdraw_amount: u64,
        min_validator_stake: u64,
        ctx: &TxContext,
    ) {
        let validator_address = tx_context::sender(ctx);
        let validator = get_validator_mut(&mut self.active_validators, validator_address);
        validator::request_withdraw_stake(validator, withdraw_amount, min_validator_stake);
        self.next_epoch_validators = derive_next_epoch_validators(self);
    }

    public(friend) fun is_active_validator(
        self: &ValidatorSet,
        validator_address: address,
    ): bool {
        option::is_some(&find_validator(&self.active_validators, validator_address))
    }

    public(friend) fun request_add_delegation(
        self: &mut ValidatorSet,
        validator_address: address,
        delegate_amount: u64,
    ) {
        let validator = get_validator_mut(&mut self.active_validators, validator_address);
        validator::request_add_delegation(validator, delegate_amount);
        self.next_epoch_validators = derive_next_epoch_validators(self);
    }

    public(friend) fun request_remove_delegation(
        self: &mut ValidatorSet,
        validator_address: address,
        delegate_amount: u64,
    ) {
        let validator_index_opt = find_validator(&self.active_validators, validator_address);
        // It's OK to not be able to find the validator. This can happen if the delegated
        // validator is no longer active.
        if (option::is_some(&validator_index_opt)) {
            let validator_index = option::extract(&mut validator_index_opt);
            let validator = vector::borrow_mut(&mut self.active_validators, validator_index);
            validator::request_remove_delegation(validator, delegate_amount);
        } else {
            // TODO: How do we deal with undelegating from inactive validators?
            // https://github.com/MystenLabs/sui/issues/2837
            abort 0
        };
        self.next_epoch_validators = derive_next_epoch_validators(self);
    }

    public(friend) fun create_epoch_records(
        self: &ValidatorSet,
        epoch: u64,
        computation_charge: u64,
        total_stake: u64,
        ctx: &mut TxContext,
    ) {
        let length = vector::length(&self.active_validators);
        let i = 0;
        while (i < length) {
            let v = vector::borrow(&self.active_validators, i);
            epoch_reward_record::create(
                epoch,
                computation_charge,
                total_stake,
                validator::delegator_count(v),
                validator::sui_address(v),
                ctx,
            );
            i = i + 1;
        }
    }

    /// Update the validator set at the end of epoch.
    /// It does the following things:
    ///   1. Distribute stake award.
    ///   2. Process pending stake deposits and withdraws for each validator (`adjust_stake`).
    ///   3. Process pending validator application and withdraws.
    ///   4. At the end, we calculate the total stake for the new epoch.
    public(friend) fun advance_epoch(
        self: &mut ValidatorSet,
        computation_reward: &mut Balance<SUI>,
        ctx: &mut TxContext,
    ) {
        // `compute_reward_distribution` must be called before `adjust_stake` to make sure we are using the current
        // epoch's stake information to compute reward distribution.
        let rewards = compute_reward_distribution(
            &self.active_validators,
            self.validator_stake,
            balance::value(computation_reward),
        );

        // `adjust_stake` must be called before `distribute_reward`, because reward distribution goes to
        // each validator's pending stake, and that shouldn't be available in the next epoch.
        adjust_stake(&mut self.active_validators, ctx);

        distribute_reward(&mut self.active_validators, &rewards, computation_reward);

        process_pending_validators(&mut self.active_validators, &mut self.pending_validators);

        process_pending_removals(&mut self.active_validators, &mut self.pending_removals, ctx);

        self.next_epoch_validators = derive_next_epoch_validators(self);

        let (validator_stake, delegation_stake, quorum_stake_threshold) = calculate_total_stake_and_quorum_threshold(&self.active_validators);
        self.validator_stake = validator_stake;
        self.delegation_stake = delegation_stake;
        self.quorum_stake_threshold = quorum_stake_threshold;
    }

    public fun validator_stake(self: &ValidatorSet): u64 {
        self.validator_stake
    }

    public fun delegation_stake(self: &ValidatorSet): u64 {
        self.delegation_stake
    }

    /// Checks whether a duplicate of `new_validator` is already in `validators`.
    /// Two validators duplicate if they share the same sui_address or same IP or same name.
    fun contains_duplicate_validator(validators: &vector<Validator>, new_validator: &Validator): bool {
        let len = vector::length(validators);
        let i = 0;
        while (i < len) {
            let v = vector::borrow(validators, i);
            if (validator::is_duplicate(v, new_validator)) {
                return true
            };
            i = i + 1;
        };
        false
    }

    /// Find validator by `validator_address`, in `validators`.
    /// Returns (true, index) if the validator is found, and the index is its index in the list.
    /// If not found, returns (false, 0).
    fun find_validator(validators: &vector<Validator>, validator_address: address): Option<u64> {
        let length = vector::length(validators);
        let i = 0;
        while (i < length) {
            let v = vector::borrow(validators, i);
            if (validator::sui_address(v) == validator_address) {
                return option::some(i)
            };
            i = i + 1;
        };
        option::none()
    }

    fun get_validator_mut(
        validators: &mut vector<Validator>,
        validator_address: address,
    ): &mut Validator {
        let validator_index_opt = find_validator(validators, validator_address);
        assert!(option::is_some(&validator_index_opt), 0);
        let validator_index = option::extract(&mut validator_index_opt);
        vector::borrow_mut(validators, validator_index)
    }

    /// Process the pending withdraw requests. For each pending request, the validator
    /// is removed from `validators` and sent back to the address of the validator.
    fun process_pending_removals(
        validators: &mut vector<Validator>, withdraw_list: &mut vector<u64>, ctx: &mut TxContext
    ) {
        sort_removal_list(withdraw_list);
        while (!vector::is_empty(withdraw_list)) {
            let index = vector::pop_back(withdraw_list);
            let validator = vector::remove(validators, index);
            validator::destroy(validator, ctx);
        }
    }

    /// Process the pending new validators. They are simply inserted into `validators`.
    fun process_pending_validators(
        validators: &mut vector<Validator>, pending_validators: &mut vector<Validator>
    ) {
        while (!vector::is_empty(pending_validators)) {
            let v = vector::pop_back(pending_validators);
            vector::push_back(validators, v);
        }
    }

    /// Sort all the pending removal indexes.
    fun sort_removal_list(withdraw_list: &mut vector<u64>) {
        let length = vector::length(withdraw_list);
        let i = 1;
        while (i < length) {
            let cur = *vector::borrow(withdraw_list, i);
            let j = i;
            while (j > 0) {
                j = j - 1;
                if (*vector::borrow(withdraw_list, j) > cur) {
                    vector::swap(withdraw_list, j, j + 1);
                } else {
                    break
                };
            };
            i = i + 1;
        };
    }

    /// Calculate the total active stake, and the amount of stake to reach quorum.
    fun calculate_total_stake_and_quorum_threshold(validators: &vector<Validator>): (u64, u64, u64) {
        let validator_state = 0;
        let delegate_stake = 0;
        let length = vector::length(validators);
        let i = 0;
        while (i < length) {
            let v = vector::borrow(validators, i);
            validator_state = validator_state + validator::stake_amount(v);
            delegate_stake = delegate_stake + validator::delegate_amount(v);
            i = i + 1;
        };
        let total_stake = validator_state + delegate_stake;
        (validator_state, delegate_stake, (total_stake + 1) * 2 / 3)
    }

    /// Calculate the required percentage threshold to reach quorum.
    /// With 3f + 1 validators, we can tolerate up to f byzantine ones.
    /// Hence (2f + 1) / total is our threshold.
    fun calculate_quorum_threshold(validators: &vector<Validator>): u8 {
        let count = vector::length(validators);
        let threshold = (2 * count / 3 + 1) * 100 / count;
        (threshold as u8)
    }

    /// Process the pending stake changes for each validator.
    fun adjust_stake(validators: &mut vector<Validator>, ctx: &mut TxContext) {
        let length = vector::length(validators);
        let i = 0;
        while (i < length) {
            let validator = vector::borrow_mut(validators, i);
            validator::adjust_stake(validator, ctx);
            i = i + 1;
        }
    }

    /// Given the current list of active validators, the total stake and total reward,
    /// calculate the amount of reward each validator should get.
    /// Returns the amount of reward for each validator, as well as a remaining reward
    /// due to integer division loss.
    fun compute_reward_distribution(
        validators: &vector<Validator>,
        total_stake: u64,
        total_reward: u64,
    ): vector<u64> {
        let results = vector::empty();
        let length = vector::length(validators);
        let i = 0;
        while (i < length) {
            let validator = vector::borrow(validators, i);
            // Integer divisions will truncate the results. Because of this, we expect that at the end
            // there will be some reward remaining in `total_reward`.
            // Use u128 to avoid multiplication overflow.
            let stake_amount: u128 = (validator::stake_amount(validator) as u128);
            let reward_amount = stake_amount * (total_reward as u128) / (total_stake as u128);
            vector::push_back(&mut results, (reward_amount as u64));
            i = i + 1;
        };
        results
    }

    // TODO: Allow reward compunding for delegators.
    fun distribute_reward(validators: &mut vector<Validator>, rewards: &vector<u64>, reward: &mut Balance<SUI>) {
        let length = vector::length(validators);
        let i = 0;
        while (i < length) {
            let validator = vector::borrow_mut(validators, i);
            let reward_amount = *vector::borrow(rewards, i);
            let reward = balance::split(reward, reward_amount);
            // Because reward goes to pending stake, it's the same as calling `request_add_stake`.
            validator::request_add_stake(validator, reward);
            i = i + 1;
        }
    }

    /// Upon any change to the validator set, derive and update the metadata of the validators for the new epoch.
    /// TODO: If we want to enforce a % on stake threshold, this is the function to do it.
    fun derive_next_epoch_validators(self: &ValidatorSet): vector<ValidatorMetadata> {
        let active_count = vector::length(&self.active_validators);
        let removal_count = vector::length(&self.pending_removals);
        let result = vector::empty();
        while (active_count > 0) {
            if (removal_count > 0) {
                let removal_index = *vector::borrow(&self.pending_removals, removal_count - 1);
                if (removal_index == active_count - 1) {
                    // This validator will be removed, and hence we won't add it to the new validator set.
                    removal_count = removal_count - 1;
                    active_count = active_count - 1;
                    continue
                };
            };
            let metadata = validator::metadata(
                vector::borrow(&self.active_validators, active_count - 1),
            );
            vector::push_back(&mut result, *metadata);
            active_count = active_count - 1;
        };
        result
    }

    #[test_only]
    public fun destroy_for_testing(
        self: ValidatorSet,
        ctx: &mut TxContext
    ) {
        let ValidatorSet {
            validator_stake: _,
            delegation_stake: _,
            quorum_stake_threshold: _,
            active_validators,
            pending_validators,
            pending_removals: _,
            next_epoch_validators: _,
        } = self;
        while (!vector::is_empty(&active_validators)) {
            let v = vector::pop_back(&mut active_validators);
            validator::destroy(v, ctx);
        };
        vector::destroy_empty(active_validators);
        vector::destroy_empty(pending_validators);
    }
}
