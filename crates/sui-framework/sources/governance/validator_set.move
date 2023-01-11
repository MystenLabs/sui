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
    use sui::staking_pool::{Self, Delegation, PendingWithdrawEntry, PoolTokenExchangeRate, StakedSui};
    use sui::epoch_time_lock::EpochTimeLock;
    use sui::priority_queue as pq;
    use sui::vec_map::{Self, VecMap};
    use sui::vec_set::{Self, VecSet};
    use sui::table_vec::{Self, TableVec};
    use sui::event;

    friend sui::sui_system;

    #[test_only]
    friend sui::validator_set_tests;

    struct ValidatorSet has store {
        /// Total amount of stake from all active validators (not including delegation),
        /// at the beginning of the epoch.
        total_validator_stake: u64,

        /// Total amount of stake from delegation, at the beginning of the epoch.
        total_delegation_stake: u64,

        /// Sum of voting power of validators.
        total_voting_power: u64,

        /// The amount of accumulated voting power to reach a quorum among all active validators.
        /// This is always 2/3 of total voting power. Keep it here to reduce potential inconsistencies
        /// among validators.
        quorum_threshold: u64,

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
        /// TODO: This is currently not used. We may use it latter for enforcing min/max stake.
        next_epoch_validators: vector<ValidatorMetadata>,

        /// Delegation switches requested during the current epoch, processed at epoch boundaries
        /// so that all the rewards with be added to the new delegation.
        pending_delegation_switches: VecMap<ValidatorPair, TableVec<PendingWithdrawEntry>>,
    }

    struct ValidatorPair has store, copy, drop {
        from: address,
        to: address,
    }

    /// Event emitted when a new delegation request is received.
    struct DelegationRequestEvent has copy, drop {
        validator_address: address,
        delegator_address: address,
        epoch: u64,
        amount: u64,
    }

    /// Event containing staking and rewards related information of
    /// each validator, emitted during epoch advancement.
    struct ValidatorEpochInfo has copy, drop {
        epoch: u64,
        validator_address: address,
        reference_gas_survey_quote: u64,
        validator_stake: u64,
        delegated_stake: u64,
        commission_rate: u64,
        stake_rewards: u64,
        pool_token_exchange_rate: PoolTokenExchangeRate,
        tallying_rule_reporters: vector<address>,
        tallying_rule_global_score: u64,
    }

    const BASIS_POINT_DENOMINATOR: u128 = 10000;

    // Errors
    const ENON_VALIDATOR_IN_REPORT_RECORDS: u64 = 0;
    const EINVALID_STAKE_ADJUSTMENT_AMOUNT: u64 = 1;

    // ==== initialization at genesis ====

    public(friend) fun new(init_active_validators: vector<Validator>): ValidatorSet {
        let (total_validator_stake, total_delegation_stake) =
            calculate_total_stakes(&init_active_validators);
        let (total_voting_power, quorum_threshold) =
            calculate_total_voting_power_and_quorum_threshold(&init_active_validators);
        let validators = ValidatorSet {
            total_validator_stake,
            total_delegation_stake,
            total_voting_power,
            quorum_threshold,
            active_validators: init_active_validators,
            pending_validators: vector::empty(),
            pending_removals: vector::empty(),
            next_epoch_validators: vector::empty(),
            pending_delegation_switches: vec_map::empty(),
        };
        validators.next_epoch_validators = derive_next_epoch_validators(&validators);
        update_validator_voting_power(&mut validators);
        validators
    }


    // ==== functions to add or remove validators ====

    /// Called by `sui_system`, add a new validator to `pending_validators`, which will be
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

    /// Called by `sui_system`, to remove a validator.
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


    // ==== staking related functions ====

    /// Called by `sui_system`, to add more stake to a validator.
    /// The new stake will be added to the validator's pending stake, which will be processed
    /// at the end of epoch.
    /// TODO: impl max stake requirement.
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

    /// Called by `sui_system`, to withdraw stake from a validator.
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

    /// Called by `sui_system`, to add a new delegation to the validator.
    /// This request is added to the validator's staking pool's pending delegation entries, processed at the end
    /// of the epoch.
    /// TODO: impl max stake requirement.
    public(friend) fun request_add_delegation(
        self: &mut ValidatorSet,
        validator_address: address,
        delegated_stake: Balance<SUI>,
        locking_period: Option<EpochTimeLock>,
        ctx: &mut TxContext,
    ) {
        let validator = get_validator_mut(&mut self.active_validators, validator_address);
        let delegator_address = tx_context::sender(ctx);
        let amount = balance::value(&delegated_stake);
        validator::request_add_delegation(validator, delegated_stake, locking_period, tx_context::sender(ctx), ctx);
        self.next_epoch_validators = derive_next_epoch_validators(self);
        event::emit(
            DelegationRequestEvent {
                validator_address,
                delegator_address,
                epoch: tx_context::epoch(ctx),
                amount,
            }
        );
    }

    /// Called by `sui_system`, to withdraw some share of a delegation from the validator. The share to withdraw
    /// is denoted by `principal_withdraw_amount`.
    /// This request is added to the validator's staking pool's pending delegation withdraw entries, processed at the end
    /// of the epoch.
    public(friend) fun request_withdraw_delegation(
        self: &mut ValidatorSet,
        delegation: &mut Delegation,
        staked_sui: &mut StakedSui,
        principal_withdraw_amount: u64,
        ctx: &mut TxContext,
    ) {
        let validator_address = staking_pool::validator_address(staked_sui);
        let validator_index_opt = find_validator(&self.active_validators, validator_address);

        assert!(option::is_some(&validator_index_opt), 0);

        let validator_index = option::extract(&mut validator_index_opt);
        let validator = vector::borrow_mut(&mut self.active_validators, validator_index);
        validator::request_withdraw_delegation(validator, delegation, staked_sui, principal_withdraw_amount, ctx);
        self.next_epoch_validators = derive_next_epoch_validators(self);
    }

    /// Called by `sui_system`, to switch some share of a delegation from one validator to another.
    /// The amount to switch is denoted by `switch_pool_token_amount`.
    /// Both the principal and reward portions of the withdrawn delegation should be added to the
    /// new validator's staking pool. We do that in two parts in this function. We first withdraw the
    /// principal portion from the current staking pool and call `request_add_delegation` to add the
    /// principal SUI to the new staking pool. The amount of rewards to switch is only known at the
    /// end of the epoch, so we bookkeep the switch requests in `pending_delegation_switches`, and
    /// process them in `advance_epoch` by calling `process_pending_delegation_switches` at epoch changes.
    public(friend) fun request_switch_delegation(
        self: &mut ValidatorSet,
        delegation: &mut Delegation,
        staked_sui: &mut StakedSui,
        new_validator_address: address,
        switch_pool_token_amount: u64,
        ctx: &mut TxContext,
    ) {
        let current_validator_address = staking_pool::validator_address(staked_sui);

        // check that the validators are not the same and they are both active.
        assert!(current_validator_address != new_validator_address, 0);
        assert!(is_active_validator(self, new_validator_address), 0);

        // withdraw principal from the current validator's pool
        let current_validator = get_validator_mut(&mut self.active_validators, current_validator_address);
        let (current_validator_pool_token, principal_stake, time_lock) =
            staking_pool::withdraw_from_principal(validator::get_staking_pool_mut_ref(current_validator), delegation, staked_sui, switch_pool_token_amount);
        let principal_sui_amount = balance::value(&principal_stake);
        validator::decrease_next_epoch_delegation(current_validator, principal_sui_amount);

        // and deposit into the new validator's pool
        request_add_delegation(self, new_validator_address, principal_stake, time_lock, ctx);

        let delegator = tx_context::sender(ctx);

        // add pending switch entry, to be processed at epoch boundaries.
        let key = ValidatorPair { from: current_validator_address, to: new_validator_address };
        let entry = staking_pool::new_pending_withdraw_entry(delegator,principal_sui_amount, current_validator_pool_token);
        if (!vec_map::contains(&self.pending_delegation_switches, &key)) {
            vec_map::insert(&mut self.pending_delegation_switches, key, table_vec::singleton(entry, ctx));
        } else {
            let entries = vec_map::get_mut(&mut self.pending_delegation_switches, &key);
            table_vec::push_back(entries, entry);
        };

        self.next_epoch_validators = derive_next_epoch_validators(self);
    }

    // ==== validator config setting functions ====

    public(friend) fun request_set_gas_price(
        self: &mut ValidatorSet,
        new_gas_price: u64,
        ctx: &TxContext,
    ) {
        let validator_address = tx_context::sender(ctx);
        let validator = get_validator_mut(&mut self.active_validators, validator_address);
        validator::request_set_gas_price(validator, new_gas_price);
    }

    public(friend) fun request_set_commission_rate(
        self: &mut ValidatorSet,
        new_commission_rate: u64,
        ctx: &TxContext,
    ) {
        let validator_address = tx_context::sender(ctx);
        let validator = get_validator_mut(&mut self.active_validators, validator_address);
        validator::request_set_commission_rate(validator, new_commission_rate);
    }


    // ==== epoch change functions ====

    /// Update the validator set at the end of epoch.
    /// It does the following things:
    ///   1. Distribute stake award.
    ///   2. Process pending stake deposits and withdraws for each validator (`adjust_stake`).
    ///   3. Process pending delegation switches, deposits, and withdraws.
    ///   4. Process pending validator application and withdraws.
    ///   5. At the end, we calculate the total stake for the new epoch.
    public(friend) fun advance_epoch(
        new_epoch: u64,
        self: &mut ValidatorSet,
        computation_reward: &mut Balance<SUI>,
        storage_fund_reward: &mut Balance<SUI>,
        validator_report_records: &mut VecMap<address, VecSet<address>>,
        reward_slashing_threshold_bps: u64,
        reward_slashing_rate: u64,
        ctx: &mut TxContext,
    ) {
        // Use the report records for the epoch to compute validators that will be
        // punished and the sum of their stakes.
        let (slashed_validators, total_slashed_validator_stake) = 
            process_and_empty_validator_report_records(
                self,
                validator_report_records,
                reward_slashing_threshold_bps,
            );

        // Compute the stake adjustments of slashed validators, to be taken into
        // account in reward computation.
        let (total_adjustment, individual_adjustments) = 
            compute_stake_adjustments(
                self,
                slashed_validators,
                reward_slashing_rate,
            );

        // `compute_reward_distribution` must be called before `distribute_reward` and `adjust_stake_and_gas_price` to 
        // make sure we are using the current epoch's stake information to compute reward distribution.
        let reward_amounts = compute_reward_distribution(
            &self.active_validators,
            self.total_validator_stake + self.total_delegation_stake,
            balance::value(computation_reward),
            total_adjustment,
            individual_adjustments,
            total_slashed_validator_stake,
        );

        // TODO: use `validator_report_records` and punish validators whose numbers of reports receives are greater than
        // some threshold.
        // Distribute the rewards before adjusting stake so that we immediately start compounding
        // the rewards for validators and delegators.
        distribute_reward(
            &mut self.active_validators, 
            &reward_amounts, 
            computation_reward,
            storage_fund_reward, 
            ctx
        );

        adjust_stake_and_gas_price(&mut self.active_validators);

        // Delegation switches must be processed before delegation deposits and withdraws so that the
        // rewards portion of the delegation switch can be added to the new validator's pool when we
        // process pending delegations.
        process_pending_delegation_switches(self, ctx);

        process_pending_delegations_and_withdraws(&mut self.active_validators, ctx);

        // Emit events after we have processed all the rewards distribution and pending delegations.
        emit_validator_epoch_events(new_epoch, &self.active_validators, &reward_amounts, validator_report_records);

        process_pending_validators(&mut self.active_validators, &mut self.pending_validators);

        process_pending_removals(self, ctx);

        // Update the voting power of each validator, now that the pending validator additions
        // and the removals have been processed.
        update_validator_voting_power(self);

        self.next_epoch_validators = derive_next_epoch_validators(self);

        let (validator_stake, delegation_stake) = calculate_total_stakes(&self.active_validators);
        self.total_validator_stake = validator_stake;
        self.total_delegation_stake = delegation_stake;

        let (total_voting_power, quorum_threshold) =
            calculate_total_voting_power_and_quorum_threshold(&self.active_validators);
        self.total_voting_power = total_voting_power;
        self.quorum_threshold = quorum_threshold;
    }

    // TODO: implement this to correctly cap the voting power.
    fun update_validator_voting_power(self: &mut ValidatorSet) {
        let num_validators = vector::length(&self.active_validators);
        let i = 0;
        while (i < num_validators) {
            let validator_mut = vector::borrow_mut(&mut self.active_validators, i);
            let updated_voting_power = validator::total_stake(validator_mut);
            validator::set_voting_power(validator_mut, updated_voting_power);
            i = i + 1;
        };
    }

    /// Called by `sui_system` to derive reference gas price for the new epoch.
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
        let threshold = self.total_voting_power - self.quorum_threshold;
        let result = 0;
        while (sum < threshold) {
            let (gas_price, stake) = pq::pop_max(&mut pq);
            result = gas_price;
            sum = sum + stake;
        };
        result
    }

    // ==== getter functions ====

    public fun total_voting_power(self: &ValidatorSet): u64 {
        self.total_voting_power
    }

    public fun total_validator_stake(self: &ValidatorSet): u64 {
        self.total_validator_stake
    }

    public fun total_delegation_stake(self: &ValidatorSet): u64 {
        self.total_delegation_stake
    }

    public fun validator_total_stake_amount(self: &ValidatorSet, validator_address: address): u64 {
        let validator = get_validator_ref(&self.active_validators, validator_address);
        validator::total_stake_amount(validator)
    }

    public fun validator_stake_amount(self: &ValidatorSet, validator_address: address): u64 {
        let validator = get_validator_ref(&self.active_validators, validator_address);
        validator::stake_amount(validator)
    }

    public fun validator_delegate_amount(self: &ValidatorSet, validator_address: address): u64 {
        let validator = get_validator_ref(&self.active_validators, validator_address);
        validator::delegate_amount(validator)
    }

    /// Get the total number of validators in the next epoch.
    public(friend) fun next_epoch_validator_count(self: &ValidatorSet): u64 {
        vector::length(&self.next_epoch_validators)
    }

    /// Returns true iff `validator_address` is a member of the active validators.
    public(friend) fun is_active_validator(
        self: &ValidatorSet,
        validator_address: address,
    ): bool {
        option::is_some(&find_validator(&self.active_validators, validator_address))
    }


    // ==== private helpers ====

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

    /// Go through all the delegation switches, withdraws the rewards portion of the switched stake from
    /// the `from` validator's pool, and deposits it into the `to` validator's pool.
    fun process_pending_delegation_switches(self: &mut ValidatorSet, ctx: &mut TxContext) {
        // for each pair of (from, to) validators, complete the delegation switch
        while (!vec_map::is_empty(&self.pending_delegation_switches)) {
            let (ValidatorPair { from, to }, entries) = vec_map::pop(&mut self.pending_delegation_switches);
            let from_validator = get_validator_mut(&mut self.active_validators, from);
            let from_pool = validator::get_staking_pool_mut_ref(from_validator);
            // withdraw rewards from the old validator's pool
            let (delegators, rewards, rewards_withdraw_amount) =
                staking_pool::batch_withdraw_rewards_and_burn_pool_tokens(from_pool, entries);
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

    /// Process all active validators' pending delegation deposits and withdraws.
    fun process_pending_delegations_and_withdraws(validators: &mut vector<Validator>, ctx: &mut TxContext) {
        let length = vector::length(validators);
        let i = 0;
        while (i < length) {
            let validator = vector::borrow_mut(validators, i);
            validator::process_pending_delegations_and_withdraws(validator, ctx);
            i = i + 1;
        }
    }

    /// Calculate the total active validator and delegated stake.
    fun calculate_total_stakes(validators: &vector<Validator>): (u64, u64) {
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
        (validator_state, delegate_stake)
    }

    /// Calculate the total voting power, and the amount of voting power to reach quorum.
    fun calculate_total_voting_power_and_quorum_threshold(validators: &vector<Validator>): (u64, u64) {
        let total_voting_power = 0;
        let length = vector::length(validators);
        let i = 0;
        while (i < length) {
            let v = vector::borrow(validators, i);
            total_voting_power = total_voting_power + validator::voting_power(v);
            i = i + 1;
        };
        (total_voting_power, (total_voting_power + 1) * 2 / 3)
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

    /// Some validators' rewards may get slashed due to getting sub-par scores
    /// for tallying rule. This function computes, using the report record, the
    /// stake adjustment amounts of non-performant validators. When computing reward
    /// distribution, the adjusted stake amount will be used so these validators
    /// receive less rewards than what's proportional to their original stake.
    fun compute_stake_adjustments(
        self: &ValidatorSet,
        slashed_validators: vector<address>,
        reward_slashing_rate: u64,
    ): (u64, VecMap<address, u64>) {
        let total_adjustment = 0;
        let individual_adjustments = vec_map::empty();
        while (!vector::is_empty(&mut slashed_validators)) {
            let validator_address = vector::pop_back(&mut slashed_validators);
            let original_stake = validator_total_stake_amount(self, validator_address);
            let adjustment_u128 = 
                (original_stake as u128) * (reward_slashing_rate as u128) 
                / BASIS_POINT_DENOMINATOR;
            let adjustment = (adjustment_u128 as u64);
            vec_map::insert(&mut individual_adjustments, validator_address, adjustment);
            total_adjustment = total_adjustment + adjustment;
        };
        (total_adjustment, individual_adjustments)
    }

    /// Empties the validator report records of the epoch and returns the addresses of the
    /// non-performant validators according to the input threshold. 
    fun process_and_empty_validator_report_records(
        self: &ValidatorSet,
        validator_report_records: &mut VecMap<address, VecSet<address>>,
        reward_slashing_threshold_bps: u64,
    ): (vector<address>, u64) {
        let num_validators = vector::length(&self.active_validators);
        // `num_validators` can't be greater than 400 so no overflow can happen below.
        let reward_slashing_threshold = (num_validators * reward_slashing_threshold_bps) / (BASIS_POINT_DENOMINATOR as u64);
        let slashed_validators = vector[];
        let sum_of_stake = 0;
        while (!vec_map::is_empty(validator_report_records)) {
            let (validator_address, reporters) = vec_map::pop(validator_report_records);
            assert!(
                is_active_validator(self, validator_address), 
                ENON_VALIDATOR_IN_REPORT_RECORDS
            );
            let num_reporters = vec_set::size(&reporters);
            if (num_reporters >= reward_slashing_threshold) {
                sum_of_stake = sum_of_stake + validator_total_stake_amount(self, validator_address);
                vector::push_back(&mut slashed_validators, validator_address);
            }
        };
        (slashed_validators, sum_of_stake)
    }

    /// Given the current list of active validators, the total stake and total reward,
    /// calculate the amount of reward each validator should get.
    /// Returns the amount of reward for each validator, as well as a remaining reward
    /// due to integer division loss.
    fun compute_reward_distribution(
        validators: &vector<Validator>,
        total_stake: u64,
        total_reward: u64,
        total_stake_adjustment: u64,
        stake_adjustments: VecMap<address, u64>,
        total_slashed_validator_stake: u64,
    ): vector<u64> {
        assert!(total_stake > total_stake_adjustment, EINVALID_STAKE_ADJUSTMENT_AMOUNT);
        let total_unslashed_validator_stake = total_stake - total_slashed_validator_stake;
        let reward_amounts = vector::empty();
        let length = vector::length(validators);
        let i = 0;
        while (i < length) {
            let validator = vector::borrow(validators, i);
            let validator_address = validator::sui_address(validator);
            // Integer divisions will truncate the results. Because of this, we expect that at the end
            // there will be some reward remaining in `total_reward`.
            // Use u128 to avoid multiplication overflow.
            let stake_amount: u128 = (validator::total_stake_amount(validator) as u128);
            let adjusted_stake_amount = 
                // If the validator is one of the slashed ones, then subtract the adjustment.
                if (vec_map::contains(&stake_adjustments, &validator_address)) {
                    let adjustment = *vec_map::get(&stake_adjustments, &validator_address);
                    stake_amount - (adjustment as u128)
                } else {
                    // Otherwise the slashed rewards should be distributed among the unslashed
                    // validators so add the corresponding adjustment.
                    let adjustment = (total_stake_adjustment as u128) * stake_amount
                                   / (total_unslashed_validator_stake as u128);
                    stake_amount + adjustment
                };
            let reward_amount = adjusted_stake_amount * (total_reward as u128) / (total_stake as u128);
            vector::push_back(&mut reward_amounts, (reward_amount as u64));
            i = i + 1;
        };
        reward_amounts
    }

    fun distribute_reward(
        validators: &mut vector<Validator>,
        reward_amounts: &vector<u64>,
        rewards: &mut Balance<SUI>,
        storage_fund_reward: &mut Balance<SUI>,
        ctx: &mut TxContext
    ) {
        let length = vector::length(validators);
        assert!(length > 0, 0);
        let storage_fund_reward_per_validator = balance::value(storage_fund_reward) / length;
        let i = 0;
        while (i < length) {
            let validator = vector::borrow_mut(validators, i);
            let reward_amount = *vector::borrow(reward_amounts, i);
            let combined_stake = validator::total_stake_amount(validator);
            let self_stake = validator::stake_amount(validator);
            let validator_reward_amount = (reward_amount as u128) * (self_stake as u128) / (combined_stake as u128);
            let validator_reward = balance::split(rewards, (validator_reward_amount as u64));
            
            let delegator_reward_amount = reward_amount - (validator_reward_amount as u64);
            let delegator_reward = balance::split(rewards, delegator_reward_amount);

            // Validator takes a cut of the rewards as commission.
            let commission_amount = (delegator_reward_amount as u128) * (validator::commission_rate(validator) as u128) / BASIS_POINT_DENOMINATOR;
            balance::join(&mut validator_reward, balance::split(&mut delegator_reward, (commission_amount as u64)));
            // Each validator gets an equal share of the storage fund rewards.
            balance::join(&mut validator_reward, balance::split(storage_fund_reward, storage_fund_reward_per_validator));
            // Add rewards to the validator.
            validator::request_add_stake(validator, validator_reward, option::none(), ctx);
            // Add rewards to delegation staking pool to auto compound for delegators.
            validator::deposit_delegation_rewards(validator, delegator_reward);
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

    /// Emit events containing information of each validator for the epoch,
    /// including stakes, rewards, performance, etc.
    fun emit_validator_epoch_events(
        new_epoch: u64,
        vs: &vector<Validator>,
        reward_amounts: &vector<u64>,
        report_records: &VecMap<address, VecSet<address>>,
    ) {
        let num_validators = vector::length(vs);
        let i = 0;
        while (i < num_validators) {
            let v = vector::borrow(vs, i);
            let validator_address = validator::sui_address(v);
            let tallying_rule_reporters =
                if (vec_map::contains(report_records, &validator_address)) {
                    vec_set::into_keys(*vec_map::get(report_records, &validator_address))
                } else {
                    vector[]
                };
            event::emit(
                ValidatorEpochInfo {
                    epoch: new_epoch,
                    validator_address,
                    reference_gas_survey_quote: validator::gas_price(v),
                    validator_stake: validator::stake_amount(v),
                    delegated_stake: validator::delegate_amount(v),
                    commission_rate: validator::commission_rate(v),
                    stake_rewards: *vector::borrow(reward_amounts, i),
                    pool_token_exchange_rate: validator::pool_token_exchange_rate(v),
                    tallying_rule_reporters,
                    // TODO: placeholder global score
                    tallying_rule_global_score: 1,
                }
            );
            i = i + 1;
        }
    }

    /// Return the active validators in `self`
    public fun active_validators(self: &ValidatorSet): &vector<Validator> {
        &self.active_validators
    }

    #[test_only]
    public fun destroy_for_testing(
        self: ValidatorSet,
    ) {
        let ValidatorSet {
            total_validator_stake: _,
            total_delegation_stake: _,
            total_voting_power: _,
            quorum_threshold: _,
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
