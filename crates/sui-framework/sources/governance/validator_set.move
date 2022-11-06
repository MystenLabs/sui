// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::validator_set {
    use std::option::{Self, Option};
    use std::vector;

    use sui::balance::{Self, Balance};
    use sui::sui::SUI;
    use sui::tx_context::{Self, TxContext};
    use sui::validator::{Self, Validator, ValidatorMetadata};
    use sui::stake::Stake;
    use sui::staking_pool::{Self, Delegation, PendingWithdrawEntry, StakedSui };
    use sui::epoch_time_lock::EpochTimeLock;
    use sui::priority_queue as pq;
    use sui::vec_map::{Self, VecMap};
    use sui::vec_set::VecSet;

    friend sui::sui_system;

    #[test_only]
    friend sui::validator_set_tests;

    struct ValidatorSet has store {
        /// Total amount of stake from all active validators (not including delegation),
        /// at the beginning of the epoch.
        total_validator_stake: u64,

        /// Total amount of stake from delegation, at the beginning of the epoch.
        total_delegation_stake: u64,

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

        /// Delegation switches requested during the current epoch, processed at epoch boundaries
        /// so that all the rewards with be added to the new delegation.
        pending_delegation_switches: VecMap<ValidatorPair, vector<PendingWithdrawEntry>>,
    }

    struct ValidatorPair has store, copy, drop {
        from: address,
        to: address,
    }

    const BASIS_POINT_DENOMINATOR: u128 = 10000;

    public(friend) fun new(init_active_validators: vector<Validator>): ValidatorSet {
        let (total_validator_stake, total_delegation_stake, quorum_stake_threshold) = calculate_total_stake_and_quorum_threshold(&init_active_validators);
        let validators = ValidatorSet {
            total_validator_stake,
            total_delegation_stake,
            quorum_stake_threshold,
            active_validators: init_active_validators,
            pending_validators: vector::empty(),
            pending_removals: vector::empty(),
            next_epoch_validators: vector::empty(),
            pending_delegation_switches: vec_map::empty(),
        };
        validators.next_epoch_validators = derive_next_epoch_validators(&validators);
        validators
    }

    /// Get the total number of validators in the next epoch.
    public(friend) fun next_epoch_validator_count(self: &ValidatorSet): u64 {
        vector::length(&self.next_epoch_validators)
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
        coin_locked_until_epoch: Option<EpochTimeLock>,
        ctx: &mut TxContext,
    ) {
        let validator_address = tx_context::sender(ctx);
        let validator = get_validator_mut(&mut self.active_validators, validator_address);
        validator::request_add_stake(validator, new_stake, coin_locked_until_epoch, ctx);
        self.next_epoch_validators = derive_next_epoch_validators(self);
    }

    /// Called by `SuiSystem`, to withdraw stake from a validator.
    /// We send a withdraw request to the validator which will be processed at the end of epoch.
    /// The remaining stake of the validator cannot be lower than `min_validator_stake`.
    public(friend) fun request_withdraw_stake(
        self: &mut ValidatorSet,
        stake: &mut Stake,
        withdraw_amount: u64,
        min_validator_stake: u64,
        ctx: &mut TxContext,
    ) {
        let validator_address = tx_context::sender(ctx);
        let validator = get_validator_mut(&mut self.active_validators, validator_address);
        validator::request_withdraw_stake(validator, stake, withdraw_amount, min_validator_stake, ctx);
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
        delegated_stake: Balance<SUI>,
        locking_period: Option<EpochTimeLock>,
        ctx: &mut TxContext,
    ) {
        let validator = get_validator_mut(&mut self.active_validators, validator_address);
        validator::request_add_delegation(validator, delegated_stake, locking_period, tx_context::sender(ctx), ctx);
        self.next_epoch_validators = derive_next_epoch_validators(self);
    }

    public(friend) fun request_set_gas_price(
        self: &mut ValidatorSet,
        new_gas_price: u64,
        ctx: &mut TxContext,
    ) {
        let validator_address = tx_context::sender(ctx);
        let validator = get_validator_mut(&mut self.active_validators, validator_address);
        validator::request_set_gas_price(validator, new_gas_price);
    }

    public(friend) fun request_set_commission_rate(
        self: &mut ValidatorSet,
        new_commission_rate: u64,
        ctx: &mut TxContext,
    ) {
        let validator_address = tx_context::sender(ctx);
        let validator = get_validator_mut(&mut self.active_validators, validator_address);
        validator::request_set_commission_rate(validator, new_commission_rate);
    }
    
    public(friend) fun request_withdraw_delegation(
        self: &mut ValidatorSet,
        delegation: &mut Delegation,
        staked_sui: &mut StakedSui,
        withdraw_pool_token_amount: u64,
        ctx: &mut TxContext,
    ) {
        let validator_address = staking_pool::validator_address(delegation);
        let validator_index_opt = find_validator(&self.active_validators, validator_address);

        assert!(option::is_some(&validator_index_opt), 0); 
        
        let validator_index = option::extract(&mut validator_index_opt);
        let validator = vector::borrow_mut(&mut self.active_validators, validator_index);
        validator::request_withdraw_delegation(validator, delegation, staked_sui, withdraw_pool_token_amount, ctx);
        self.next_epoch_validators = derive_next_epoch_validators(self);
    }

    public(friend) fun request_switch_delegation(
        self: &mut ValidatorSet,
        delegation: &mut Delegation,
        staked_sui: &mut StakedSui,
        new_validator_address: address,
        switch_pool_token_amount: u64,
        ctx: &mut TxContext,
    ) {
        let current_validator_address = staking_pool::validator_address(delegation);

        // check that the validators are not the same and they are both active.
        assert!(current_validator_address != new_validator_address, 0);
        assert!(is_active_validator(self, new_validator_address), 0);
        let current_validator_index_opt = find_validator(&self.active_validators, current_validator_address);
        assert!(option::is_some(&current_validator_index_opt), 0); 
        
        // withdraw principal from the current validator's pool
        let current_validator_index = option::extract(&mut current_validator_index_opt);
        let current_validator = vector::borrow_mut(&mut self.active_validators, current_validator_index);
        let (current_validator_pool_token, principal_stake, time_lock) = 
            staking_pool::withdraw_principal(validator::get_staking_pool_mut_ref(current_validator), delegation, staked_sui, switch_pool_token_amount);
        let principal_sui_amount = balance::value(&principal_stake);
        validator::decrease_next_epoch_delegation(current_validator, principal_sui_amount);

        // and deposit into the new validator's pool
        request_add_delegation(self, new_validator_address, principal_stake, time_lock, ctx);

        let delegator = tx_context::sender(ctx);

        // add pending switch entry, to be processed at epoch boundaries.
        let key = ValidatorPair { from: current_validator_address, to: new_validator_address };
        let entry = staking_pool::new_pending_withdraw_entry(delegator,principal_sui_amount, current_validator_pool_token);
        if (!vec_map::contains(&self.pending_delegation_switches, &key)) {
            vec_map::insert(&mut self.pending_delegation_switches, key, vector::singleton(entry));
        } else {
            let entries = vec_map::get_mut(&mut self.pending_delegation_switches, &key);
            vector::push_back(entries, entry);
        };

        self.next_epoch_validators = derive_next_epoch_validators(self);
    }

    fun process_delegation_switches(self: &mut ValidatorSet, ctx: &mut TxContext) {
        // for each pair of (from, to) validators, complete the delegation switch
        while (!vec_map::is_empty(&self.pending_delegation_switches)) {
            let (ValidatorPair { from, to }, entries) = vec_map::pop(&mut self.pending_delegation_switches);
            let from_validator = get_validator_mut(&mut self.active_validators, from);
            let from_pool = validator::get_staking_pool_mut_ref(from_validator);
            // withdraw rewards from the old validator's pool
            let (delegators, rewards, rewards_withdraw_amount) = staking_pool::batch_rewards_withdraws(from_pool, entries);
            validator::decrease_next_epoch_delegation(from_validator, rewards_withdraw_amount);

            assert!(vector::length(&delegators) == vector::length(&rewards), 0);

            let to_validator = get_validator_mut(&mut self.active_validators, to); 
            // add delegations to the new validator
            while (!vector::is_empty(&rewards)) {
                let delegator = vector::pop_back(&mut delegators);
                let new_stake = vector::pop_back(&mut rewards);
                validator::request_add_delegation(
                    to_validator, 
                    new_stake, 
                    option::none(), // no time lock for rewards
                    delegator, 
                    ctx
                );
            };
            vector::destroy_empty(rewards);
        };
    }

    fun process_pending_delegations(validators: &mut vector<Validator>, ctx: &mut TxContext) {
        let length = vector::length(validators);
        let i = 0;
        while (i < length) {
            let validator = vector::borrow_mut(validators, i);
            validator::process_pending_delegations(validator, ctx);
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
        validator_reward: &mut Balance<SUI>,
        delegator_reward: &mut Balance<SUI>,
        _validator_report_records: &VecMap<address, VecSet<address>>,
        ctx: &mut TxContext,
    ) {
        // `compute_reward_distribution` must be called before `adjust_stake` to make sure we are using the current
        // epoch's stake information to compute reward distribution.
        let (validator_reward_amounts, delegator_reward_amounts) = compute_reward_distribution(
            &self.active_validators,
            self.total_validator_stake,
            balance::value(validator_reward),
            self.total_delegation_stake,
            balance::value(delegator_reward),
        );

        // `adjust_stake_and_gas_price` must be called before `distribute_reward`, because reward distribution goes to
        // each validator's pending stake, and that shouldn't be available in the next epoch.
        adjust_stake_and_gas_price(&mut self.active_validators);

        // TODO: use `validator_report_records` and punish validators whose numbers of reports receives are greater than
        // some threshold.
        distribute_reward(
            &mut self.active_validators, 
            &validator_reward_amounts, 
            validator_reward,
            &delegator_reward_amounts,
            delegator_reward, 
            ctx
        );

        process_delegation_switches(self, ctx);

        process_pending_delegations(&mut self.active_validators, ctx);

        process_pending_validators(&mut self.active_validators, &mut self.pending_validators);

        process_pending_removals(self, ctx);

        self.next_epoch_validators = derive_next_epoch_validators(self);

        let (validator_stake, delegation_stake, quorum_stake_threshold) = calculate_total_stake_and_quorum_threshold(&self.active_validators);
        self.total_validator_stake = validator_stake;
        self.total_delegation_stake = delegation_stake;
        self.quorum_stake_threshold = quorum_stake_threshold;
    }

    /// Derive the reference gas price based on the gas price quote submitted by each validator.
    /// The returned gas price should be greater than or equal to 2/3 of the validators submitted
    /// gas price, weighted by stake.
    public fun derive_reference_gas_price(self: &ValidatorSet): u64 {
        let vs = &self.active_validators;
        let num_validators = vector::length(vs);
        let entries = vector::empty();
        let i = 0;
        while (i < num_validators) {
            let v = vector::borrow(vs, i);
            vector::push_back(
                &mut entries, 
                // Count both self and delegated stake
                pq::new_entry(validator::gas_price(v), validator::stake_amount(v) + validator::delegate_amount(v))
            );
            i = i + 1;
        };
        // Build a priority queue that will pop entries with gas price from the highest to the lowest.
        let pq = pq::new(entries);
        let sum = 0;
        let threshold = (total_validator_stake(self) + total_delegation_stake(self)) / 3;
        let result = 0;
        while (sum < threshold) {
            let (gas_price, stake) = pq::pop_max(&mut pq);
            result = gas_price;
            sum = sum + stake;
        };
        result
    }

    public fun total_validator_stake(self: &ValidatorSet): u64 {
        self.total_validator_stake
    }

    public fun total_delegation_stake(self: &ValidatorSet): u64 {
        self.total_delegation_stake
    }

    public fun validator_stake_amount(self: &ValidatorSet, validator_address: address): u64 {
        let validator = get_validator_ref(&self.active_validators, validator_address);
        validator::stake_amount(validator)
    }

    public fun validator_delegate_amount(self: &ValidatorSet, validator_address: address): u64 {
        let validator = get_validator_ref(&self.active_validators, validator_address);
        validator::delegate_amount(validator)
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

    fun get_validator_ref(
        validators: &vector<Validator>,
        validator_address: address,
    ): &Validator {
        let validator_index_opt = find_validator(validators, validator_address);
        assert!(option::is_some(&validator_index_opt), 0);
        let validator_index = option::extract(&mut validator_index_opt);
        vector::borrow(validators, validator_index)
    }

    /// Process the pending withdraw requests. For each pending request, the validator
    /// is removed from `validators` and sent back to the address of the validator.
    fun process_pending_removals(
        self: &mut ValidatorSet,
        ctx: &mut TxContext,
    ) {
        sort_removal_list(&mut self.pending_removals);
        while (!vector::is_empty(&self.pending_removals)) {
            let index = vector::pop_back(&mut self.pending_removals);
            let validator = vector::remove(&mut self.active_validators, index);
            self.total_delegation_stake = self.total_delegation_stake - validator::delegate_amount(&validator);
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
    fun adjust_stake_and_gas_price(validators: &mut vector<Validator>) {
        let length = vector::length(validators);
        let i = 0;
        while (i < length) {
            let validator = vector::borrow_mut(validators, i);
            validator::adjust_stake_and_gas_price(validator);
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
        total_delegation_stake: u64,
        total_delegation_reward: u64,
    ): (vector<u64>, vector<u64>) {
        let validator_reward_amounts = vector::empty();
        let delegator_reward_amounts = vector::empty();
        let length = vector::length(validators);
        let i = 0;
        while (i < length) {
            let validator = vector::borrow(validators, i);
            // Integer divisions will truncate the results. Because of this, we expect that at the end
            // there will be some reward remaining in `total_reward`.
            // Use u128 to avoid multiplication overflow.
            let stake_amount: u128 = (validator::stake_amount(validator) as u128);
            let reward_amount = stake_amount * (total_reward as u128) / (total_stake as u128);
            vector::push_back(&mut validator_reward_amounts, (reward_amount as u64));

            let delegation_stake_amount: u128 = (validator::delegate_amount(validator) as u128);
            let delegation_reward_amount = 
                if (total_delegation_stake == 0) 0
                else delegation_stake_amount * (total_delegation_reward as u128) / (total_delegation_stake as u128);
            vector::push_back(&mut delegator_reward_amounts, (delegation_reward_amount as u64));

            i = i + 1;
        };
        (validator_reward_amounts, delegator_reward_amounts)
    }

    fun distribute_reward(
        validators: &mut vector<Validator>,
        validator_reward_amounts: &vector<u64>,
        validator_rewards: &mut Balance<SUI>,
        delegator_reward_amounts: &vector<u64>,
        delegator_rewards: &mut Balance<SUI>,
        ctx: &mut TxContext
    ) {
        let length = vector::length(validators);
        let i = 0;
        while (i < length) {
            let validator = vector::borrow_mut(validators, i);
            let validator_reward_amount = *vector::borrow(validator_reward_amounts, i);
            let validator_reward = balance::split(validator_rewards, validator_reward_amount);
            
            let delegator_reward_amount = *vector::borrow(delegator_reward_amounts, i);
            let delegator_reward = balance::split(delegator_rewards, delegator_reward_amount);

            // Validator takes a cut of the rewards as commission.
            let commission_amount = (delegator_reward_amount as u128) * (validator::commission_rate(validator) as u128) / BASIS_POINT_DENOMINATOR;
            balance::join(&mut validator_reward, balance::split(&mut delegator_reward, (commission_amount as u64)));

            // Add rewards to the validator. Because reward goes to pending stake, it's the same as calling `request_add_stake`.
            validator::request_add_stake(validator, validator_reward, option::none(), ctx);
            // Add rewards to delegation staking pool to auto compound for delegators.
            validator::distribute_rewards(validator, delegator_reward, ctx);
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
        let i = 0;
        let pending_count = vector::length(&self.pending_validators);
        while (i < pending_count) {
            let metadata = validator::metadata(
                vector::borrow(&self.pending_validators, i),
            );
            vector::push_back(&mut result, *metadata);
            i = i + 1;
        };
        result
    }

    #[test_only]
    public fun destroy_for_testing(
        self: ValidatorSet,
    ) {
        let ValidatorSet {
            total_validator_stake: _,
            total_delegation_stake: _,
            quorum_stake_threshold: _,
            active_validators,
            pending_validators,
            pending_removals: _,
            next_epoch_validators: _,
            pending_delegation_switches,
        } = self;
        while (!vector::is_empty(&active_validators)) {
            let v = vector::pop_back(&mut active_validators);
            validator::destroy(v, &mut tx_context::dummy());
        };
        vector::destroy_empty(active_validators);
        vector::destroy_empty(pending_validators);
        vec_map::destroy_empty(pending_delegation_switches);
    }
}
