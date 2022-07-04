// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::validator_set {
    use std::vector;

    use sui::balance::{Self, Balance};
    use sui::epoch_reward_record;
    use sui::sui::SUI;
    use sui::tx_context::{Self, TxContext};
    use sui::validator::{Self, Validator, ValidatorMetadata};
    use sui::vec_map::{Self, VecMap};

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
        active_validators: VecMap<address, Validator>,

        /// List of new validator candidates added during the current epoch.
        /// They will be processed at the end of the epoch.
        pending_validators: VecMap<address, Validator>,

        /// Removal requests from the validators. Each element is the address of a validator in
        /// `active_validators`. There are no duplicates
        pending_removals: vector<address>,

        /// The metadata of the validator set for the next epoch.
        // TODO: this should be kept up-to-date--every time a change request is received,
        // this set should be updated. For now, it only gets updated at epoch boundaries.
        next_epoch_validators: VecMap<address, ValidatorMetadata>,
    }

    public(friend) fun new(init_active_validators: VecMap<address,Validator>): ValidatorSet {
        let (validator_stake, delegation_stake, quorum_stake_threshold) = calculate_total_stake_and_quorum_threshold(&init_active_validators);
        let validators = ValidatorSet {
            validator_stake,
            delegation_stake,
            quorum_stake_threshold,
            active_validators: init_active_validators,
            pending_validators: vec_map::empty(),
            pending_removals: vector::empty(),
            next_epoch_validators: vec_map::empty(),
        };
        derive_next_epoch_validators_(&mut validators);
        validators
    }

    /// Get the total number of candidates that might become validators in the next epoch.
    public(friend) fun total_validator_candidate_count(self: &ValidatorSet): u64 {
        vec_map::size(&self.active_validators)
            + vec_map::size(&self.pending_validators)
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
        vec_map::insert(&mut self.pending_validators, validator::sui_address(&validator), validator);
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
        assert!(is_active_validator(self, &validator_address), 0);
        assert!(
            !vector::contains(&self.pending_removals, &validator_address),
            0
        );
        vector::push_back(&mut self.pending_removals, validator_address);
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
        let current_validator = vec_map::get_mut(&mut self.active_validators, &validator_address);
        let next_epoch_metadata = vec_map::get_mut(&mut self.next_epoch_validators, &validator_address);

        validator::request_add_stake(current_validator, next_epoch_metadata, new_stake)
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
        let current_validator = vec_map::get_mut(&mut self.active_validators, &validator_address);
        let next_epoch_metadata = vec_map::get_mut(&mut self.next_epoch_validators, &validator_address);

        validator::request_withdraw_stake(
            current_validator, 
            next_epoch_metadata, 
            withdraw_amount,
            min_validator_stake
        )
    }

    public(friend) fun is_active_validator(
        self: &ValidatorSet,
        validator_address: &address,
    ): bool {
        vec_map::contains(&self.active_validators, validator_address)
    }

    public(friend) fun request_add_delegation(
        self: &mut ValidatorSet,
        validator_address: &address,
        delegate_amount: u64,
    ) {
        let validator = vec_map::get_mut(&mut self.active_validators, validator_address);
        let next_epoch_metadata = vec_map::get_mut(&mut self.next_epoch_validators, validator_address);
        validator::request_add_delegation(validator, next_epoch_metadata, delegate_amount)
    }

    public(friend) fun request_remove_delegation(
        self: &mut ValidatorSet,
        validator_address: &address,
        delegate_amount: u64,
    ) {
        // It's OK to not be able to find the validator. This can happen if the delegated
        // validator is no longer active.
        let validators = &mut self.active_validators;
        if (vec_map::contains(validators, validator_address)) {
            let validator = vec_map::get_mut(validators, validator_address);
            let next_epoch_metadata = vec_map::get_mut(&mut self.next_epoch_validators, validator_address);
            validator::request_remove_delegation(validator, next_epoch_metadata, delegate_amount);
        } else {
            // TODO: How do we deal with undelegating from inactive validators?
            // https://github.com/MystenLabs/sui/issues/2837
            abort 0
        }
    }

    public(friend) fun create_epoch_records(
        self: &ValidatorSet,
        epoch: u64,
        computation_charge: u64,
        total_stake: u64,
        ctx: &mut TxContext,
    ) {
        let length = vec_map::size(&self.active_validators);
        let i = 0;
        while (i < length) {
            let (addr, v) = vec_map::get_entry_by_idx(&self.active_validators, i);
            epoch_reward_record::create(
                epoch,
                computation_charge,
                total_stake,
                validator::delegator_count(v),
                *addr,
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

        derive_next_epoch_validators(self, ctx);
        // pending_validators and pending_removals are now empty and should not be read anymore

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
    fun contains_duplicate_validator(validators: &VecMap<address,Validator>, new_validator: &Validator): bool {
        let len = vec_map::size(validators);
        let i = 0;
        while (i < len) {
            let (_k, v) = vec_map::get_entry_by_idx(validators, i);
            if (validator::is_duplicate(v, new_validator)) {
                return true
            };
            i = i + 1;
        };
        false
    }

    /// Upon any change to the validator set, derive and update the metadata of the validators for the new epoch.
    /// `to_add` and `to_remove` will both be empty after this function.
    /// TODO: If we want to enforce a % on stake threshold, this is the function to do it.
    fun derive_next_epoch_validators(self: &mut ValidatorSet, ctx: &mut TxContext) {
        let validators = &mut self.active_validators;
        // process the add requests
        vec_map::disjoint_union(validators, &mut self.pending_validators);
        // Process the pending withdraw requests. For each pending request, the `Validator` object
        // is removed from `validators` and sent back to the address of the validator.
        let to_remove = &mut self.pending_removals;
        while (!vector::is_empty(to_remove)) {
            let addr = vector::pop_back(to_remove);
            let validator = vec_map::remove_value(validators, &addr);
            validator::destroy(validator, ctx);
        };
        // copy over leftovers after removal into `result`
        derive_next_epoch_validators_(self)
    }

    // copy the active validator set into `next_epoch_validators`
    fun derive_next_epoch_validators_(self: &mut ValidatorSet) {
        self.next_epoch_validators = vec_map::empty();
        let i = 0;
        let len = vec_map::size(&self.active_validators);
        while (i < len) {
            let (addr, validator) = vec_map::get_entry_by_idx(&self.active_validators, i);
            vec_map::insert(&mut self.next_epoch_validators, *addr, *validator::metadata(validator));
            i = i + 1;
        }
    }

    /// Calculate the total active stake, and the amount of stake to reach quorum.
    fun calculate_total_stake_and_quorum_threshold(validators: &VecMap<address,Validator>): (u64, u64, u64) {
        let validator_state = 0;
        let delegate_stake = 0;
        let length = vec_map::size(validators);
        let i = 0;
        while (i < length) {
            let (_addr, v) = vec_map::get_entry_by_idx(validators, i);
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
    fun adjust_stake(validators: &mut VecMap<address, Validator>, ctx: &mut TxContext) {
        let length = vec_map::size(validators);
        let i = 0;
        while (i < length) {
            let (_addr, validator) = vec_map::get_entry_by_idx_mut(validators, i);
            validator::adjust_stake(validator, ctx);
            i = i + 1;
        }
    }

    /// Given the current list of active validators, the total stake and total reward,
    /// calculate the amount of reward each validator should get.
    /// Returns the amount of reward for each validator, as well as a remaining reward
    /// due to integer division loss.
    fun compute_reward_distribution(
        validators: &VecMap<address,Validator>,
        total_stake: u64,
        total_reward: u64,
    ): vector<u64> {
        let results = vector::empty();
        let length = vec_map::size(validators);
        let i = 0;
        while (i < length) {
            let (_addr, validator) = vec_map::get_entry_by_idx(validators, i);
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
    fun distribute_reward(validators: &mut VecMap<address,Validator>, rewards: &vector<u64>, reward: &mut Balance<SUI>) {
        let length = vec_map::size(validators);
        let i = 0;
        while (i < length) {
            let (_addr, validator) = vec_map::get_entry_by_idx_mut(validators, i);
            let reward_amount = *vector::borrow(rewards, i);
            let reward = balance::split(reward, reward_amount);
            // Because reward goes to pending stake, it's the same as calling `add_stake`.
            // TODO: do we want to add this to the epoch metadata?
            validator::add_stake(validator, reward);
            i = i + 1;
        }
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
        let (_addresses, validators) = vec_map::into_keys_values(active_validators);
        while (!vector::is_empty(&validators)) {
            let v = vector::pop_back(&mut validators);
            validator::destroy(v, ctx);
        };
        vector::destroy_empty(validators);
        vec_map::destroy_empty(pending_validators)
    }
}
