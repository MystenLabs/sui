// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::validator_set {
    use std::option::{Self, Option};
    use std::vector;

    use sui::balance::{Self, Balance};
    use sui::sui::SUI;
    use sui::tx_context::{Self, TxContext};
    use sui::validator::{Self, Validator, staking_pool_id, sui_address};
    use sui::staking_pool::{PoolTokenExchangeRate, StakedSui, pool_id};
    use sui::epoch_time_lock::EpochTimeLock;
    use sui::object::ID;
    use sui::priority_queue as pq;
    use sui::vec_map::{Self, VecMap};
    use sui::vec_set::{Self, VecSet};
    use sui::table::{Self, Table};
    use sui::event;
    use sui::table_vec::{Self, TableVec};
    use sui::voting_power;

    friend sui::sui_system;

    #[test_only]
    friend sui::validator_set_tests;

    struct ValidatorSet has store {
        /// Total amount of stake from all active validators at the beginning of the epoch.
        total_stake: u64,

        /// The current list of active validators.
        active_validators: vector<Validator>,

        /// List of new validator candidates added during the current epoch.
        /// They will be processed at the end of the epoch.
        pending_validators: TableVec<Validator>,

        /// Removal requests from the validators. Each element is an index
        /// pointing to `active_validators`.
        pending_removals: vector<u64>,

        /// Mappings from staking pool's ID to the sui address of a validator.
        staking_pool_mappings: Table<ID, address>,
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
        stake: u64,
        commission_rate: u64,
        stake_rewards: u64,
        pool_token_exchange_rate: PoolTokenExchangeRate,
        tallying_rule_reporters: vector<address>,
        tallying_rule_global_score: u64,
    }

    const BASIS_POINT_DENOMINATOR: u128 = 10000;

    // Errors
    const ENonValidatorInReportRecords: u64 = 0;
    const EInvalidStakeAdjustmentAmount: u64 = 1;
    const EDuplicateValidator: u64 = 2;

    // ==== initialization at genesis ====

    public(friend) fun new(init_active_validators: vector<Validator>, ctx: &mut TxContext): ValidatorSet {
        let total_stake = calculate_total_stakes(&init_active_validators);
        let staking_pool_mappings = table::new(ctx);
        let num_validators = vector::length(&init_active_validators);
        let i = 0;
        while (i < num_validators) {
            let validator = vector::borrow(&init_active_validators, i);
            table::add(&mut staking_pool_mappings, staking_pool_id(validator), sui_address(validator));
            i = i + 1;
        };
        let validators = ValidatorSet {
            total_stake,
            active_validators: init_active_validators,
            pending_validators: table_vec::empty(ctx),
            pending_removals: vector::empty(),
            staking_pool_mappings,
        };
        voting_power::set_voting_power(&mut validators.active_validators);
        validators
    }


    // ==== functions to add or remove validators ====

    /// Called by `sui_system`, add a new validator to `pending_validators`, which will be
    /// processed at the end of epoch.
    public(friend) fun request_add_validator(self: &mut ValidatorSet, validator: Validator) {
        assert!(
            !is_currently_active_validator(self, &validator)
                && !is_currently_pending_validator(self, &validator),
            EDuplicateValidator
        );
        table_vec::push_back(&mut self.pending_validators, validator);
    }

    /// Called by `sui_system`, to remove a validator.
    /// The index of the validator is added to `pending_removals` and
    /// will be processed at the end of epoch.
    /// Only an active validator can request to be removed.
    public(friend) fun request_remove_validator(
        self: &mut ValidatorSet,
        ctx: &mut TxContext,
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
    }


    // ==== staking related functions ====

    /// Called by `sui_system`, to add a new delegation to the validator.
    /// This request is added to the validator's staking pool's pending delegation entries, processed at the end
    /// of the epoch.
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
        staked_sui: StakedSui,
        ctx: &mut TxContext,
    ) {
        let validator_address = *table::borrow(&self.staking_pool_mappings, pool_id(&staked_sui));
        let validator_index_opt = find_validator(&self.active_validators, validator_address);

        assert!(option::is_some(&validator_index_opt), 0);

        let validator_index = option::extract(&mut validator_index_opt);
        let validator = vector::borrow_mut(&mut self.active_validators, validator_index);
        validator::request_withdraw_delegation(validator, staked_sui, ctx);
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
    ///   3. Process pending delegation deposits, and withdraws.
    ///   4. Process pending validator application and withdraws.
    ///   5. At the end, we calculate the total stake for the new epoch.
    public(friend) fun advance_epoch(
        self: &mut ValidatorSet,
        computation_reward: &mut Balance<SUI>,
        storage_fund_reward: &mut Balance<SUI>,
        validator_report_records: VecMap<address, VecSet<address>>,
        reward_slashing_rate: u64,
        ctx: &mut TxContext,
    ) {
        let new_epoch = tx_context::epoch(ctx) + 1;
        let total_stake = self.total_stake;

        // Compute the reward distribution without taking into account the tallying rule slashing.
        let (unadjusted_staking_reward_amounts, unadjusted_storage_fund_reward_amounts) = compute_unadjusted_reward_distribution(
            &self.active_validators,
            total_stake,
            balance::value(computation_reward),
            balance::value(storage_fund_reward),
        );

        // Use the tallying rule report records for the epoch to compute validators that will be
        // punished and the sum of their stakes.
        let (slashed_validators, total_slashed_validator_stake) =
            compute_slashed_validators_and_total_stake(
                self,
                copy validator_report_records,
            );

        // Compute the reward adjustments of slashed validators, to be taken into
        // account in adjusted reward computation.
        let (total_staking_reward_adjustment, individual_staking_reward_adjustments,
             total_storage_fund_reward_adjustment, individual_storage_fund_reward_adjustments
            ) =
            compute_reward_adjustments(
                get_validator_indices(&self.active_validators, &slashed_validators),
                reward_slashing_rate,
                &unadjusted_staking_reward_amounts,
                &unadjusted_storage_fund_reward_amounts,
            );

        // Compute the adjusted amounts of stake each validator should get given the tallying rule
        // reward adjustments we computed before.
        // `compute_adjusted_reward_distribution` must be called before `distribute_reward` and `adjust_stake_and_gas_price` to
        // make sure we are using the current epoch's stake information to compute reward distribution.
        let (adjusted_staking_reward_amounts, adjusted_storage_fund_reward_amounts) = compute_adjusted_reward_distribution(
            &self.active_validators,
            total_stake,
            total_slashed_validator_stake,
            unadjusted_staking_reward_amounts,
            unadjusted_storage_fund_reward_amounts,
            total_staking_reward_adjustment,
            individual_staking_reward_adjustments,
            total_storage_fund_reward_adjustment,
            individual_storage_fund_reward_adjustments
        );

        // Distribute the rewards before adjusting stake so that we immediately start compounding
        // the rewards for validators and delegators.
        distribute_reward(
            &mut self.active_validators,
            &adjusted_staking_reward_amounts,
            &adjusted_storage_fund_reward_amounts,
            computation_reward,
            storage_fund_reward,
            ctx
        );

        adjust_stake_and_gas_price(&mut self.active_validators);

        process_pending_delegations_and_withdraws(&mut self.active_validators, ctx);

        // Emit events after we have processed all the rewards distribution and pending delegations.
        emit_validator_epoch_events(new_epoch, &self.active_validators, &adjusted_staking_reward_amounts,
            &validator_report_records, &slashed_validators);

        process_pending_validators(self);

        process_pending_removals(self, ctx);

        self.total_stake = calculate_total_stakes(&self.active_validators);

        voting_power::set_voting_power(&mut self.active_validators);
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
                pq::new_entry(validator::gas_price(v), validator::voting_power(v))
            );
            i = i + 1;
        };
        // Build a priority queue that will pop entries with gas price from the highest to the lowest.
        let pq = pq::new(entries);
        let sum = 0;
        let threshold = voting_power::total_voting_power() - voting_power::quorum_threshold();
        let result = 0;
        while (sum < threshold) {
            let (gas_price, stake) = pq::pop_max(&mut pq);
            result = gas_price;
            sum = sum + stake;
        };
        result
    }

    // ==== getter functions ====

    public fun total_stake(self: &ValidatorSet): u64 {
        self.total_stake
    }

    public fun validator_total_stake_amount(self: &ValidatorSet, validator_address: address): u64 {
        let validator = get_validator_ref(&self.active_validators, validator_address);
        validator::total_stake_amount(validator)
    }

    public fun validator_delegate_amount(self: &ValidatorSet, validator_address: address): u64 {
        let validator = get_validator_ref(&self.active_validators, validator_address);
        validator::delegate_amount(validator)
    }

    public fun validator_staking_pool_id(self: &ValidatorSet, validator_address: address): ID {
        let validator = get_validator_ref(&self.active_validators, validator_address);
        validator::staking_pool_id(validator)
    }

    public fun staking_pool_mappings(self: &ValidatorSet): &Table<ID, address> {
        &self.staking_pool_mappings
    }

    /// Get the total number of validators in the next epoch.
    public(friend) fun next_epoch_validator_count(self: &ValidatorSet): u64 {
        vector::length(&self.active_validators) - vector::length(&self.pending_removals) + table_vec::length(&self.pending_validators)
    }

    /// Returns true iff `validator_address` is a member of the active validators.
    public(friend) fun is_active_validator(
        self: &ValidatorSet,
        validator_address: address,
    ): bool {
        option::is_some(&find_validator(&self.active_validators, validator_address))
    }


    // ==== private helpers ====

    /// Checks whether `new_validator` is already in currently active validator list.
    /// Two validators are identical if they share the same sui_address or same IP or same name.
    fun is_currently_active_validator(self: &ValidatorSet, new_validator: &Validator): bool {
        let len = vector::length(&self.active_validators);
        let i = 0;
        while (i < len) {
            let v = vector::borrow(&self.active_validators, i);
            if (validator::is_duplicate(v, new_validator)) {
                return true
            };
            i = i + 1;
        };
        false
    }

    /// Checks whether `new_validator` is already in currently pending validator list.
    /// Two validators are identical if they share the same sui_address or same IP or same name.
    fun is_currently_pending_validator(self: &ValidatorSet, new_validator: &Validator): bool {
        let len = table_vec::length(&self.pending_validators);
        let i = 0;
        while (i < len) {
            let v = table_vec::borrow(&self.pending_validators, i);
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

    /// Given a vector of validator addresses, return their indices in the validator set.
    /// Aborts if any address isn't in the given validator set.
    fun get_validator_indices(validators: &vector<Validator>, validator_addresses: &vector<address>): vector<u64> {
        let length = vector::length(validator_addresses);
        let i = 0;
        let res = vector[];
        while (i < length) {
            let addr = *vector::borrow(validator_addresses, i);
            let index_opt = find_validator(validators, addr);
            assert!(option::is_some(&index_opt), 0);
            vector::push_back(&mut res, option::destroy_some(index_opt));
            i = i + 1;
        };
        res
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

    #[test_only]
    public fun get_validator_ref_test(
        validators: &ValidatorSet,
        validator_address: address,
    ): &Validator {
        get_validator_ref(&validators.active_validators, validator_address)
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
            table::remove(&mut self.staking_pool_mappings, staking_pool_id(&validator));
            self.total_stake = self.total_stake - validator::total_stake_amount(&validator);
            validator::destroy(validator, ctx);
        }
    }

    /// Process the pending new validators. They are simply inserted into `validators`.
    fun process_pending_validators(
        self: &mut ValidatorSet,
    ) {
        while (!table_vec::is_empty(&self.pending_validators)) {
            let validator = table_vec::pop_back(&mut self.pending_validators);
            table::add(&mut self.staking_pool_mappings, staking_pool_id(&validator), sui_address(&validator));
            vector::push_back(&mut self.active_validators, validator);
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

    /// Process all active validators' pending delegation deposits and withdraws.
    fun process_pending_delegations_and_withdraws(
        validators: &mut vector<Validator>, ctx: &mut TxContext
    ) {
        let length = vector::length(validators);
        let i = 0;
        while (i < length) {
            let validator = vector::borrow_mut(validators, i);
            validator::process_pending_delegations_and_withdraws(validator, ctx);
            i = i + 1;
        }
    }

    /// Calculate the total active validator stake.
    fun calculate_total_stakes(validators: &vector<Validator>): u64 {
        let stake = 0;
        let length = vector::length(validators);
        let i = 0;
        while (i < length) {
            let v = vector::borrow(validators, i);
            stake = stake + validator::total_stake_amount(v);
            i = i + 1;
        };
        stake
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

    /// Compute both the individual reward adjustments and total reward adjustment for staking rewards
    /// as well as storage fund rewards.
    fun compute_reward_adjustments(
        slashed_validator_indices: vector<u64>,
        reward_slashing_rate: u64,
        unadjusted_staking_reward_amounts: &vector<u64>,
        unadjusted_storage_fund_reward_amounts: &vector<u64>,
    ): (
        u64, // sum of staking reward adjustments
        VecMap<u64, u64>, // mapping of individual validator's staking reward adjustment from index -> amount
        u64, // sum of storage fund reward adjustments
        VecMap<u64, u64>, // mapping of individual validator's storage fund reward adjustment from index -> amount
    ) {
        let total_staking_reward_adjustment = 0;
        let individual_staking_reward_adjustments = vec_map::empty();
        let total_storage_fund_reward_adjustment = 0;
        let individual_storage_fund_reward_adjustments = vec_map::empty();

        while (!vector::is_empty(&mut slashed_validator_indices)) {
            let validator_index = vector::pop_back(&mut slashed_validator_indices);

            // Use the slashing rate to compute the amount of staking rewards slashed from this punished validator.
            let unadjusted_staking_reward = *vector::borrow(unadjusted_staking_reward_amounts, validator_index);
            let staking_reward_adjustment_u128 =
                (unadjusted_staking_reward as u128) * (reward_slashing_rate as u128)
                / BASIS_POINT_DENOMINATOR;

            // Insert into individual mapping and record into the total adjustment sum.
            vec_map::insert(&mut individual_staking_reward_adjustments, validator_index, (staking_reward_adjustment_u128 as u64));
            total_staking_reward_adjustment = total_staking_reward_adjustment + (staking_reward_adjustment_u128 as u64);

            // Do the same thing for storage fund rewards.
            let unadjusted_storage_fund_reward = *vector::borrow(unadjusted_storage_fund_reward_amounts, validator_index);
            let storage_fund_reward_adjustment_u128 =
                (unadjusted_storage_fund_reward as u128) * (reward_slashing_rate as u128)
                / BASIS_POINT_DENOMINATOR;
            vec_map::insert(&mut individual_storage_fund_reward_adjustments, validator_index, (storage_fund_reward_adjustment_u128 as u64));
            total_storage_fund_reward_adjustment = total_storage_fund_reward_adjustment + (storage_fund_reward_adjustment_u128 as u64);
        };

        (
            total_staking_reward_adjustment, individual_staking_reward_adjustments,
            total_storage_fund_reward_adjustment, individual_storage_fund_reward_adjustments
        )
    }

    /// Process the validator report records of the epoch and return the addresses of the
    /// non-performant validators according to the input threshold.
    fun compute_slashed_validators_and_total_stake(
        self: &ValidatorSet,
        validator_report_records: VecMap<address, VecSet<address>>,
    ): (vector<address>, u64) {
        let slashed_validators = vector[];
        let sum_of_stake = 0;
        while (!vec_map::is_empty(&validator_report_records)) {
            let (validator_address, reporters) = vec_map::pop(&mut validator_report_records);
            assert!(
                is_active_validator(self, validator_address),
                ENonValidatorInReportRecords,
            );
            // Sum up the voting power of validators that have reported this validator and check if it has
            // passed the slashing threshold.
            let reporter_votes = sum_voting_power_by_addresses(&self.active_validators, &vec_set::into_keys(reporters));
            if (reporter_votes >= voting_power::quorum_threshold()) {
                sum_of_stake = sum_of_stake + validator_total_stake_amount(self, validator_address);
                vector::push_back(&mut slashed_validators, validator_address);
            }
        };
        (slashed_validators, sum_of_stake)
    }

    /// Given the current list of active validators, the total stake and total reward,
    /// calculate the amount of reward each validator should get, without taking into
    /// account the tallyig rule results.
    /// Returns the unadjusted amounts of staking reward and storage fund reward for each validator.
    fun compute_unadjusted_reward_distribution(
        validators: &vector<Validator>,
        total_stake: u64,
        total_staking_reward: u64,
        total_storage_fund_reward: u64,
    ): (vector<u64>, vector<u64>) {
        let staking_reward_amounts = vector::empty();
        let storage_fund_reward_amounts = vector::empty();
        let length = vector::length(validators);
        let storage_fund_reward_per_validator = total_storage_fund_reward / length;
        let i = 0;
        while (i < length) {
            let validator = vector::borrow(validators, i);
            // Integer divisions will truncate the results. Because of this, we expect that at the end
            // there will be some reward remaining in `total_staking_reward`.
            // Use u128 to avoid multiplication overflow.
            let stake_amount: u128 = (validator::total_stake_amount(validator) as u128);
            let reward_amount = stake_amount * (total_staking_reward as u128) / (total_stake as u128);
            vector::push_back(&mut staking_reward_amounts, (reward_amount as u64));
            // Storage fund's share of the rewards are equally distributed among validators.
            vector::push_back(&mut storage_fund_reward_amounts, storage_fund_reward_per_validator);
            i = i + 1;
        };
        (staking_reward_amounts, storage_fund_reward_amounts)
    }

    /// Use the reward adjustment info to compute the adjusted rewards each validator should get.
    /// Returns the staking rewards each validator gets and the storage fund rewards each validator gets.
    /// The staking rewards are shared with the delegators while the storage fund ones are not.
    fun compute_adjusted_reward_distribution(
        validators: &vector<Validator>,
        total_stake: u64,
        total_slashed_validator_stake: u64,
        unadjusted_staking_reward_amounts: vector<u64>,
        unadjusted_storage_fund_reward_amounts: vector<u64>,
        total_staking_reward_adjustment: u64,
        individual_staking_reward_adjustments: VecMap<u64, u64>,
        total_storage_fund_reward_adjustment: u64,
        individual_storage_fund_reward_adjustments: VecMap<u64, u64>,
    ): (vector<u64>, vector<u64>) {
        let total_unslashed_validator_stake = total_stake - total_slashed_validator_stake;
        let adjusted_staking_reward_amounts = vector::empty();
        let adjusted_storage_fund_reward_amounts = vector::empty();

        let length = vector::length(validators);
        let num_unslashed_validators = length - vec_map::size(&individual_staking_reward_adjustments);

        let i = 0;
        while (i < length) {
            let validator = vector::borrow(validators, i);
            // Integer divisions will truncate the results. Because of this, we expect that at the end
            // there will be some reward remaining in `total_reward`.
            // Use u128 to avoid multiplication overflow.
            let stake_amount: u128 = (validator::total_stake_amount(validator) as u128);

            // Compute adjusted staking reward.
            let unadjusted_staking_reward_amount = *vector::borrow(&unadjusted_staking_reward_amounts, i);
            let adjusted_staking_reward_amount =
                // If the validator is one of the slashed ones, then subtract the adjustment.
                if (vec_map::contains(&individual_staking_reward_adjustments, &i)) {
                    let adjustment = *vec_map::get(&individual_staking_reward_adjustments, &i);
                    unadjusted_staking_reward_amount - adjustment
                } else {
                    // Otherwise the slashed rewards should be distributed among the unslashed
                    // validators so add the corresponding adjustment.
                    let adjustment = (total_staking_reward_adjustment as u128) * stake_amount
                                   / (total_unslashed_validator_stake as u128);
                    unadjusted_staking_reward_amount + (adjustment as u64)
                };
            vector::push_back(&mut adjusted_staking_reward_amounts, adjusted_staking_reward_amount);

            // Compute adjusted storage fund reward.
            let unadjusted_storage_fund_reward_amount = *vector::borrow(&unadjusted_storage_fund_reward_amounts, i);
            let adjusted_storage_fund_reward_amount =
                // If the validator is one of the slashed ones, then subtract the adjustment.
                if (vec_map::contains(&individual_storage_fund_reward_adjustments, &i)) {
                    let adjustment = *vec_map::get(&individual_storage_fund_reward_adjustments, &i);
                    unadjusted_storage_fund_reward_amount - adjustment
                } else {
                    // Otherwise the slashed rewards should be equally distributed among the unslashed validators.
                    let adjustment = total_storage_fund_reward_adjustment / num_unslashed_validators;
                    unadjusted_storage_fund_reward_amount + adjustment
                };
            vector::push_back(&mut adjusted_storage_fund_reward_amounts, adjusted_storage_fund_reward_amount);

            i = i + 1;
        };

        (adjusted_staking_reward_amounts, adjusted_storage_fund_reward_amounts)
    }

    fun distribute_reward(
        validators: &mut vector<Validator>,
        adjusted_staking_reward_amounts: &vector<u64>,
        adjusted_storage_fund_reward_amounts: &vector<u64>,
        staking_rewards: &mut Balance<SUI>,
        storage_fund_reward: &mut Balance<SUI>,
        ctx: &mut TxContext
    ) {
        let new_epoch = tx_context::epoch(ctx) + 1;
        let length = vector::length(validators);
        assert!(length > 0, 0);
        let i = 0;
        while (i < length) {
            let validator = vector::borrow_mut(validators, i);
            let staking_reward_amount = *vector::borrow(adjusted_staking_reward_amounts, i);
            let delegator_reward = balance::split(staking_rewards, staking_reward_amount);

            // Validator takes a cut of the rewards as commission.
            let validator_commission_amount = (staking_reward_amount as u128) * (validator::commission_rate(validator) as u128) / BASIS_POINT_DENOMINATOR;

            // The validator reward = storage_fund_reward + commission.
            let validator_reward = balance::split(&mut delegator_reward, (validator_commission_amount as u64));

            // Add storage fund rewards to the validator's reward.
            balance::join(&mut validator_reward, balance::split(storage_fund_reward, *vector::borrow(adjusted_storage_fund_reward_amounts, i)));

            // Add rewards to the validator. Don't try and distribute rewards though if the payout is zero.
            if (balance::value(&validator_reward) > 0) {
                let validator_address = validator::sui_address(validator);
                validator::request_add_delegation(validator, validator_reward, option::none(), validator_address, ctx);
            } else {
                balance::destroy_zero(validator_reward);
            };

            // Add rewards to delegation staking pool to auto compound for delegators.
            validator::deposit_delegation_rewards(validator, delegator_reward, new_epoch);
            i = i + 1;
        }
    }

    /// Emit events containing information of each validator for the epoch,
    /// including stakes, rewards, performance, etc.
    fun emit_validator_epoch_events(
        new_epoch: u64,
        vs: &vector<Validator>,
        reward_amounts: &vector<u64>,
        report_records: &VecMap<address, VecSet<address>>,
        slashed_validators: &vector<address>,
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
            let tallying_rule_global_score =
                if (vector::contains(slashed_validators, &validator_address)) 0
                else 1;
            event::emit(
                ValidatorEpochInfo {
                    epoch: new_epoch,
                    validator_address,
                    reference_gas_survey_quote: validator::gas_price(v),
                    stake: validator::total_stake_amount(v),
                    commission_rate: validator::commission_rate(v),
                    stake_rewards: *vector::borrow(reward_amounts, i),
                    pool_token_exchange_rate: validator::pool_token_exchange_rate_at_epoch(v, new_epoch),
                    tallying_rule_reporters,
                    tallying_rule_global_score,
                }
            );
            i = i + 1;
        }
    }

    /// Sum up the total stake of a given list of validator addresses.
    public fun sum_voting_power_by_addresses(vs: &vector<Validator>, addresses: &vector<address>): u64 {
        let sum = 0;
        let i = 0;
        let length = vector::length(addresses);
        while (i < length) {
            let validator = get_validator_ref(vs, *vector::borrow(addresses, i));
            sum = sum + validator::voting_power(validator);
            i = i + 1;
        };
        sum
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
            total_stake: _,
            active_validators,
            pending_validators,
            pending_removals: _,
            staking_pool_mappings,
        } = self;
        destroy_validators_for_testing(active_validators);
        table_vec::destroy_empty(pending_validators);
        table::drop(staking_pool_mappings);
    }

    #[test_only]
    public fun destroy_validators_for_testing(v: vector<Validator>) {
        while (!vector::is_empty(&v)) {
            let v = vector::pop_back(&mut v);
            validator::destroy(v, &mut tx_context::dummy());
        };
        vector::destroy_empty(v)
    }
}
