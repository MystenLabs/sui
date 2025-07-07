// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui_system::validator_set;

use sui::bag::{Self, Bag};
use sui::balance::Balance;
use sui::event;
use sui::priority_queue as pq;
use sui::sui::SUI;
use sui::table::{Self, Table};
use sui::table_vec::{Self, TableVec};
use sui::vec_map::{Self, VecMap};
use sui::vec_set::VecSet;
use sui_system::staking_pool::{
    PoolTokenExchangeRate,
    StakedSui,
    pool_id,
    FungibleStakedSui,
    fungible_staked_sui_pool_id
};
use sui_system::validator::{Validator, staking_pool_id, sui_address};
use sui_system::validator_cap::{UnverifiedValidatorOperationCap, ValidatorOperationCap};
use sui_system::validator_wrapper::ValidatorWrapper;
use sui_system::voting_power;

// Errors
const ENonValidatorInReportRecords: u64 = 0;
#[allow(unused_const)]
const EInvalidStakeAdjustmentAmount: u64 = 1;
const EDuplicateValidator: u64 = 2;
const ENoPoolFound: u64 = 3;
const ENotAValidator: u64 = 4;
const EMinJoiningStakeNotReached: u64 = 5;
const EAlreadyValidatorCandidate: u64 = 6;
const EValidatorNotCandidate: u64 = 7;
const ENotValidatorCandidate: u64 = 8;
const ENotActiveOrPendingValidator: u64 = 9;
const EStakingBelowThreshold: u64 = 10;
const EValidatorAlreadyRemoved: u64 = 11;
const ENotAPendingValidator: u64 = 12;
const EValidatorSetEmpty: u64 = 13;
const EInvalidCap: u64 = 101;

// same as in sui_system
const ACTIVE_VALIDATOR_ONLY: u8 = 1;
const ACTIVE_OR_PENDING_VALIDATOR: u8 = 2;
const ANY_VALIDATOR: u8 = 3;

const BASIS_POINT_DENOMINATOR: u64 = 10000;
const MIN_STAKING_THRESHOLD: u64 = 1_000_000_000; // 1 SUI

const PHASE_LENGTH: u64 = 14; // phases are 14 days = 14 epochs

public struct ValidatorSet has store {
    /// Total amount of stake from all active validators at the beginning of the epoch.
    /// Written only once per epoch, in `advance_epoch` function.
    total_stake: u64,
    /// The current list of active validators.
    active_validators: vector<Validator>,
    /// List of new validator candidates added during the current epoch.
    /// They will be processed at the end of the epoch.
    pending_active_validators: TableVec<Validator>,
    /// Removal requests from the validators. Each element is an index
    /// pointing to `active_validators`.
    pending_removals: vector<u64>,
    /// Mappings from staking pool's ID to the sui address of a validator.
    staking_pool_mappings: Table<ID, address>,
    /// Mapping from a staking pool ID to the inactive validator that has that pool as its staking pool.
    /// When a validator is deactivated the validator is removed from `active_validators` it
    /// is added to this table so that stakers can continue to withdraw their stake from it.
    inactive_validators: Table<ID, ValidatorWrapper>,
    /// Table storing preactive/candidate validators, mapping their addresses to their `Validator ` structs.
    /// When an address calls `request_add_validator_candidate`, they get added to this table and become a preactive
    /// validator.
    /// When the candidate has met the min stake requirement, they can call `request_add_validator` to
    /// officially add them to the active validator set `active_validators` next epoch.
    validator_candidates: Table<address, ValidatorWrapper>,
    /// Table storing the number of epochs during which a validator's stake has been below the low stake threshold.
    at_risk_validators: VecMap<address, u64>,
    /// Any extra fields that's not defined statically.
    extra_fields: Bag,
}

#[allow(unused_field)]
/// Event containing staking and rewards related information of
/// each validator, emitted during epoch advancement.
public struct ValidatorEpochInfoEvent has copy, drop {
    epoch: u64,
    validator_address: address,
    reference_gas_survey_quote: u64,
    stake: u64,
    commission_rate: u64,
    pool_staking_reward: u64,
    storage_fund_staking_reward: u64,
    pool_token_exchange_rate: PoolTokenExchangeRate,
    tallying_rule_reporters: vector<address>,
    tallying_rule_global_score: u64,
}

/// V2 of ValidatorEpochInfoEvent containing more information about the validator.
public struct ValidatorEpochInfoEventV2 has copy, drop {
    epoch: u64,
    validator_address: address,
    reference_gas_survey_quote: u64,
    stake: u64,
    voting_power: u64,
    commission_rate: u64,
    pool_staking_reward: u64,
    storage_fund_staking_reward: u64,
    pool_token_exchange_rate: PoolTokenExchangeRate,
    tallying_rule_reporters: vector<address>,
    tallying_rule_global_score: u64,
}

/// Event emitted every time a new validator joins the committee.
/// The epoch value corresponds to the first epoch this change takes place.
public struct ValidatorJoinEvent has copy, drop {
    epoch: u64,
    validator_address: address,
    staking_pool_id: ID,
}

/// Event emitted every time a validator leaves the committee.
/// The epoch value corresponds to the first epoch this change takes place.
public struct ValidatorLeaveEvent has copy, drop {
    epoch: u64,
    validator_address: address,
    staking_pool_id: ID,
    is_voluntary: bool,
}

/// Key for the `extra_fields` bag to store the start epoch of allowing admission
/// of new validators based on a minimum voting power rather than a minimum stake.
public struct VotingPowerAdmissionStartEpochKey() has copy, drop, store;

// ==== initialization at genesis ====

public(package) fun new(
    init_active_validators: vector<Validator>,
    ctx: &mut TxContext,
): ValidatorSet {
    let total_stake = calculate_total_stakes(&init_active_validators);
    let mut staking_pool_mappings = table::new(ctx);
    init_active_validators.do_ref!(|v| {
        staking_pool_mappings.add(v.staking_pool_id(), v.sui_address());
    });
    let mut validators = ValidatorSet {
        total_stake,
        active_validators: init_active_validators,
        pending_active_validators: table_vec::empty(ctx),
        pending_removals: vector[],
        staking_pool_mappings,
        inactive_validators: table::new(ctx),
        validator_candidates: table::new(ctx),
        at_risk_validators: vec_map::empty(),
        extra_fields: bag::new(ctx),
    };
    voting_power::set_voting_power(&mut validators.active_validators, total_stake);
    validators
}

// ==== functions to add or remove validators ====

/// Called by `sui_system` to add a new validator candidate.
public(package) fun request_add_validator_candidate(
    self: &mut ValidatorSet,
    validator: Validator,
    ctx: &mut TxContext,
) {
    // The next assertions are not critical for the protocol, but they are here to catch problematic configs earlier.
    assert!(
        !self.is_duplicate_with_active_validator(&validator)
            && !self.is_duplicate_with_pending_validator(&validator),
        EDuplicateValidator,
    );
    let validator_address = validator.sui_address();
    assert!(!self.validator_candidates.contains(validator_address), EAlreadyValidatorCandidate);

    assert!(validator.is_preactive(), EValidatorNotCandidate);
    // Add validator to the candidates mapping and the pool id mappings so that users can start
    // staking with this candidate.
    self.staking_pool_mappings.add(validator.staking_pool_id(), validator_address);
    self.validator_candidates.add(validator.sui_address(), validator.wrap_v1(ctx));
}

/// Called by `sui_system` to remove a validator candidate, and move them to `inactive_validators`.
public(package) fun request_remove_validator_candidate(
    self: &mut ValidatorSet,
    ctx: &mut TxContext,
) {
    let validator_address = ctx.sender();
    assert!(self.validator_candidates.contains(validator_address), ENotValidatorCandidate);
    let mut validator = self.validator_candidates.remove(validator_address).destroy();
    assert!(validator.is_preactive(), EValidatorNotCandidate);

    let staking_pool_id = validator.staking_pool_id();

    // Remove the validator's staking pool from mappings.
    self.staking_pool_mappings.remove(staking_pool_id);

    // Deactivate the staking pool.
    validator.deactivate(ctx.epoch());

    // Add to the inactive tables.
    self.inactive_validators.add(staking_pool_id, validator.wrap_v1(ctx));
}

/// Called by `sui_system` to add a new validator to `pending_active_validators`, which will be
/// processed at the end of epoch.
public(package) fun request_add_validator(self: &mut ValidatorSet, ctx: &TxContext) {
    let validator_address = ctx.sender();
    assert!(self.validator_candidates.contains(validator_address), ENotValidatorCandidate);
    let validator = self.validator_candidates.remove(validator_address).destroy();
    assert!(
        !self.is_duplicate_with_active_validator(&validator)
            && !self.is_duplicate_with_pending_validator(&validator),
        EDuplicateValidator,
    );
    assert!(validator.is_preactive(), EValidatorNotCandidate);
    assert!(self.can_join(validator.total_stake(), ctx), EMinJoiningStakeNotReached);

    self.pending_active_validators.push_back(validator);
}

/// Return `true` if a  candidate validator with `stake` will have sufficeint voting power to join the validator set
fun can_join(self: &ValidatorSet, stake: u64, ctx: &TxContext): bool {
    let (min_joining_voting_power, _, _) = self.get_voting_power_thresholds(ctx);

    // if the validator will have at least `min_joining_voting_power` after joining, they can join.
    // this formula comes from SIP-39: https://github.com/sui-foundation/sips/blob/main/sips/sip-39.md
    let future_total_stake = self.total_stake + stake;
    let future_validator_voting_power = voting_power::derive_raw_voting_power(
        stake,
        future_total_stake,
    );
    future_validator_voting_power >= min_joining_voting_power
}

/// return (min, low, very low voting power) thresholds
fun get_voting_power_thresholds(self: &ValidatorSet, ctx: &TxContext): (u64, u64, u64) {
    let start_epoch = {
        let key = VotingPowerAdmissionStartEpochKey();
        if (self.extra_fields.contains(key)) self.extra_fields[key]
        else ctx.epoch() + 1 // will give us the phase 1 values
    };

    // these numbers come from SIP-39: https://github.com/sui-foundation/sips/blob/main/sips/sip-39.md
    let curr_epoch = ctx.epoch();
    if (curr_epoch < start_epoch + PHASE_LENGTH) (12, 8, 4) // phase 1
    else if (curr_epoch < start_epoch + (2 * PHASE_LENGTH)) (6, 4, 2) // phase 2
    else (3, 2, 1) // phase 3
}

public(package) fun assert_no_pending_or_active_duplicates(
    self: &ValidatorSet,
    validator: &Validator,
) {
    // Validator here must be active or pending, and thus must be identified as duplicate exactly once.
    assert!(
        count_duplicates_vec(&self.active_validators, validator) +
            count_duplicates_tablevec(&self.pending_active_validators, validator) == 1,
        EDuplicateValidator,
    );
}

/// Called by `sui_system`, to remove a validator.
/// The index of the validator is added to `pending_removals` and
/// will be processed at the end of epoch.
/// Only an active validator can request to be removed.
public(package) fun request_remove_validator(self: &mut ValidatorSet, ctx: &TxContext) {
    let validator_address = ctx.sender();
    let validator_index = find_validator(
        &self.active_validators,
        validator_address,
    ).destroy_or!(abort ENotAValidator);
    assert!(!self.pending_removals.contains(&validator_index), EValidatorAlreadyRemoved);
    self.pending_removals.push_back(validator_index);
}

// ==== staking related functions ====

/// Called by `sui_system`, to add a new stake to the validator.
/// This request is added to the validator's staking pool's pending stake entries, processed at the end
/// of the epoch.
/// Aborts in case the staking amount is smaller than MIN_STAKING_THRESHOLD
public(package) fun request_add_stake(
    self: &mut ValidatorSet,
    validator_address: address,
    stake: Balance<SUI>,
    ctx: &mut TxContext,
): StakedSui {
    let sui_amount = stake.value();
    assert!(sui_amount >= MIN_STAKING_THRESHOLD, EStakingBelowThreshold);
    self
        .get_candidate_or_active_validator_mut(validator_address)
        .request_add_stake(stake, ctx.sender(), ctx)
}

/// Called by `sui_system`, to withdraw some share of a stake from the validator. The share to withdraw
/// is denoted by `principal_withdraw_amount`. One of two things occurs in this function:
/// 1. If the `staked_sui` is staked with an active validator, the request is added to the validator's
///    staking pool's pending stake withdraw entries, processed at the end of the epoch.
/// 2. If the `staked_sui` was staked with a validator that is no longer active,
///    the stake and any rewards corresponding to it will be immediately processed.
public(package) fun request_withdraw_stake(
    self: &mut ValidatorSet,
    staked_sui: StakedSui,
    ctx: &TxContext,
): Balance<SUI> {
    let staking_pool_id = staked_sui.pool_id();
    let validator = if (self.staking_pool_mappings.contains(staking_pool_id)) {
        // This is an active validator.
        let validator_address = self.staking_pool_mappings[staked_sui.pool_id()];
        self.get_candidate_or_active_validator_mut(validator_address)
    } else {
        // This is an inactive pool.
        assert!(self.inactive_validators.contains(staking_pool_id), ENoPoolFound);
        self.inactive_validators[staking_pool_id].load_validator_maybe_upgrade()
    };
    validator.request_withdraw_stake(staked_sui, ctx)
}

public(package) fun convert_to_fungible_staked_sui(
    self: &mut ValidatorSet,
    staked_sui: StakedSui,
    ctx: &mut TxContext,
): FungibleStakedSui {
    let staking_pool_id = staked_sui.pool_id();
    let validator = if (self.staking_pool_mappings.contains(staking_pool_id)) {
        // This is an active validator.
        let validator_address = self.staking_pool_mappings[staking_pool_id];
        self.get_candidate_or_active_validator_mut(validator_address)
    } else {
        // This is an inactive pool.
        assert!(self.inactive_validators.contains(staking_pool_id), ENoPoolFound);
        self.inactive_validators[staking_pool_id].load_validator_maybe_upgrade()
    };

    validator.convert_to_fungible_staked_sui(staked_sui, ctx)
}

public(package) fun redeem_fungible_staked_sui(
    self: &mut ValidatorSet,
    fungible_staked_sui: FungibleStakedSui,
    ctx: &TxContext,
): Balance<SUI> {
    let staking_pool_id = fungible_staked_sui.pool_id();

    let validator = if (self.staking_pool_mappings.contains(staking_pool_id)) {
        // This is an active validator.
        let validator_address = self.staking_pool_mappings[staking_pool_id];
        self.get_candidate_or_active_validator_mut(validator_address)
    } else {
        // This is an inactive pool.
        assert!(self.inactive_validators.contains(staking_pool_id), ENoPoolFound);
        self.inactive_validators[staking_pool_id].load_validator_maybe_upgrade()
    };

    validator.redeem_fungible_staked_sui(fungible_staked_sui, ctx)
}

// ==== validator config setting functions ====

public(package) fun request_set_commission_rate(
    self: &mut ValidatorSet,
    new_commission_rate: u64,
    ctx: &TxContext,
) {
    let validator_address = ctx.sender();
    let validator = get_validator_mut(&mut self.active_validators, validator_address);
    validator.request_set_commission_rate(new_commission_rate);
}

// ==== epoch change functions ====

/// Update the validator set at the end of epoch.
/// It does the following things:
///   1. Distribute stake award.
///   2. Process pending stake deposits and withdraws for each validator (`adjust_stake`).
///   3. Process pending stake deposits, and withdraws.
///   4. Process pending validator application and withdraws.
///   5. At the end, we calculate the total stake for the new epoch.
public(package) fun advance_epoch(
    self: &mut ValidatorSet,
    computation_reward: &mut Balance<SUI>,
    storage_fund_reward: &mut Balance<SUI>,
    validator_report_records: &mut VecMap<address, VecSet<address>>,
    reward_slashing_rate: u64,
    low_stake_grace_period: u64,
    ctx: &mut TxContext,
) {
    let new_epoch = ctx.epoch() + 1;
    let total_voting_power = voting_power::total_voting_power();

    // switch to using voting power based admission, if we are not already using it
    let key = VotingPowerAdmissionStartEpochKey();
    if (!self.extra_fields.contains(key)) self.extra_fields.add(key, ctx.epoch());

    // Compute the reward distribution without taking into account the tallying rule slashing.
    let (
        unadjusted_staking_reward_amounts,
        unadjusted_storage_fund_reward_amounts,
    ) = compute_unadjusted_reward_distribution(
        &self.active_validators,
        total_voting_power,
        computation_reward.value(),
        storage_fund_reward.value(),
    );

    // Use the tallying rule report records for the epoch to compute validators that will be
    // punished.
    let slashed_validators = self.compute_slashed_validators(*validator_report_records);

    let total_slashed_validator_voting_power = sum_voting_power_by_addresses(
        &self.active_validators,
        &slashed_validators,
    );

    // Compute the reward adjustments of slashed validators, to be taken into
    // account in adjusted reward computation.
    let (
        total_staking_reward_adjustment,
        individual_staking_reward_adjustments,
        total_storage_fund_reward_adjustment,
        individual_storage_fund_reward_adjustments,
    ) = compute_reward_adjustments(
        get_validator_indices(&self.active_validators, &slashed_validators),
        reward_slashing_rate,
        &unadjusted_staking_reward_amounts,
        &unadjusted_storage_fund_reward_amounts,
    );

    // Compute the adjusted amounts of stake each validator should get given the tallying rule
    // reward adjustments we computed before.
    // `compute_adjusted_reward_distribution` must be called before `distribute_reward` and `adjust_stake_and_gas_price` to
    // make sure we are using the current epoch's stake information to compute reward distribution.
    let (
        adjusted_staking_reward_amounts,
        adjusted_storage_fund_reward_amounts,
    ) = compute_adjusted_reward_distribution(
        &self.active_validators,
        total_voting_power,
        total_slashed_validator_voting_power,
        unadjusted_staking_reward_amounts,
        unadjusted_storage_fund_reward_amounts,
        total_staking_reward_adjustment,
        individual_staking_reward_adjustments,
        total_storage_fund_reward_adjustment,
        individual_storage_fund_reward_adjustments,
    );

    // Distribute the rewards before adjusting stake so that we immediately start compounding
    // the rewards for validators and stakers.
    distribute_reward(
        &mut self.active_validators,
        &adjusted_staking_reward_amounts,
        &adjusted_storage_fund_reward_amounts,
        computation_reward,
        storage_fund_reward,
        ctx,
    );

    adjust_stake_and_gas_price(&mut self.active_validators);

    process_pending_stakes_and_withdraws(&mut self.active_validators, ctx);

    // Emit events after we have processed all the rewards distribution and pending stakes.
    emit_validator_epoch_events(
        new_epoch,
        &self.active_validators,
        &adjusted_staking_reward_amounts,
        &adjusted_storage_fund_reward_amounts,
        validator_report_records,
        &slashed_validators,
    );

    self.process_pending_removals(validator_report_records, ctx);

    // kick low stake validators out.
    let new_total_stake = self.update_validator_positions_and_calculate_total_stake(
        low_stake_grace_period,
        validator_report_records,
        ctx,
    );
    self.total_stake = new_total_stake;
    voting_power::set_voting_power(&mut self.active_validators, new_total_stake);

    // At this point, self.active_validators are updated for next epoch.
    // Now we process the staged validator metadata.
    self.effectuate_staged_metadata();
}

/// This function does the following:
/// - removes validators from `at_risk` group if their voting power is above the LOW threshold
/// - increments the number of epochs a validator has been below the LOW threshold but above the
///     VERY LOW threshold
/// - removes validators from the active set if they have been below the LOW threshold for more than
///     `low_stake_grace_period` epochs
/// - removes validators from the active set immediately if they are below the VERY LOW threshold
/// - activates pending validators if they have sufficient voting power
fun update_validator_positions_and_calculate_total_stake(
    self: &mut ValidatorSet,
    low_stake_grace_period: u64,
    validator_report_records: &mut VecMap<address, VecSet<address>>,
    ctx: &mut TxContext,
): u64 {
    // take all pending validators out of the tablevec and put them in a local vector
    let pending_active_validators = vector::tabulate!(
        self.pending_active_validators.length(),
        |_| self.pending_active_validators.pop_back(),
    );

    // Note: we count the total stake of pending validators as well!
    let pending_total_stake = calculate_total_stakes(&pending_active_validators);
    let initial_total_stake = calculate_total_stakes(&self.active_validators) + pending_total_stake;
    let (
        min_joining_voting_power_threshold,
        low_voting_power_threshold,
        very_low_voting_power_threshold,
    ) = self.get_voting_power_thresholds(ctx);
    // Iterate through all the active validators, record their low stake status, and kick them out if the condition is met.
    let mut total_removed_stake = 0; // amount of stake to remove due to departed_validators
    let mut i = self.active_validators.length();
    while (i > 0) {
        i = i - 1;
        let validator_ref = &self.active_validators[i];
        let validator_address = validator_ref.sui_address();
        let validator_stake = validator_ref.total_stake();

        // calculate the voting power for this validator in the next epoch if no validators are removed
        // if one of more low stake validators are removed, it's possible this validator will have higher voting power--that's ok.
        let voting_power = voting_power::derive_raw_voting_power(
            validator_stake,
            initial_total_stake,
        );

        // SIP-39: a validator can remain indefinitely with a voting power ≥ LOW_VOTING_POWER_THRESHOLD
        if (voting_power >= low_voting_power_threshold) {
            // The validator is safe. We remove their entry from the at_risk map if there exists one.
            if (self.at_risk_validators.contains(&validator_address)) {
                self.at_risk_validators.remove(&validator_address);
            }
            // SIP-39: as soon as the validator’s voting power falls to VERY_LOW_VOTING_POWER_THRESHOLD,
            //      they are on probation and must acquire sufficient stake to recover to voting power
        } else if (voting_power >= very_low_voting_power_threshold) {
            // The stake is a bit below the threshold so we increment the entry of the validator in the map.
            let new_low_stake_period = if (self.at_risk_validators.contains(&validator_address)) {
                let num_epochs = &mut self.at_risk_validators[&validator_address];
                *num_epochs = *num_epochs + 1;
                *num_epochs
            } else {
                self.at_risk_validators.insert(validator_address, 1);
                1
            };

            // If the grace period has passed, the validator has to leave us.
            if (new_low_stake_period > low_stake_grace_period) {
                let validator = self.active_validators.remove(i);
                let removed_stake = self.process_validator_departure(
                    validator,
                    validator_report_records,
                    false, // the validator is kicked out involuntarily
                    ctx,
                );
                total_removed_stake = total_removed_stake + removed_stake;
            }
            // SIP-39: at the end of an epoch when new voting powers are computed based on stake changes,
            //      any validator with VOTING_POWER < VERY_LOW_VOTING_POWER_THRESHOLD will be removed
        } else {
            // The validator's stake is lower than the very low threshold so we kick them out immediately.
            let validator = self.active_validators.remove(i);
            let removed_stake = self.process_validator_departure(
                validator,
                validator_report_records,
                false, // the validator is kicked out involuntarily
                ctx,
            );
            total_removed_stake = total_removed_stake + removed_stake;
        }
    };
    // check that pending validators still have sufficient stake to be added. this was checked at
    // the time of request_add_validator, but stake may have been withdrawn, or stakes of other
    // validators may have increased significantly
    pending_active_validators.do!(|mut validator| {
        let validator_stake = validator.total_stake();
        let voting_power = voting_power::derive_raw_voting_power(
            validator_stake,
            initial_total_stake,
        );
        if (voting_power >= min_joining_voting_power_threshold) {
            validator.activate(ctx.epoch());
            event::emit(ValidatorJoinEvent {
                epoch: ctx.epoch(),
                validator_address: validator.sui_address(),
                staking_pool_id: validator.staking_pool_id(),
            });
            self.active_validators.push_back(validator);
        } else {
            // return validator object to the candidate pool. want to do this directly instead of
            // calling request_add_validator_candidate because staking_pool_mappings already has an
            // entry for this validator, and the duplicate checks are redundant
            self
                .validator_candidates
                .add(
                    validator.sui_address(),
                    validator.wrap_v1(ctx),
                );
            total_removed_stake = total_removed_stake + validator_stake;
        }
    });

    // new total stake is the initial total minus the amount removed via validators we kicked out
    initial_total_stake - total_removed_stake
}

/// Effectuate pending next epoch metadata if they are staged.
fun effectuate_staged_metadata(self: &mut ValidatorSet) {
    self.active_validators.do_mut!(|v| v.effectuate_staged_metadata());
}

/// Called by `sui_system` to derive reference gas price for the new epoch.
/// Derive the reference gas price based on the gas price quote submitted by each validator.
/// The returned gas price should be greater than or equal to 2/3 of the validators submitted
/// gas price, weighted by stake.
public fun derive_reference_gas_price(self: &ValidatorSet): u64 {
    let entries = self
        .active_validators
        .map_ref!(|v| pq::new_entry(v.gas_price(), v.voting_power()));

    // Build a priority queue that will pop entries with gas price from the highest to the lowest.
    let mut pq = pq::new(entries);
    let mut sum = 0;
    let threshold = voting_power::total_voting_power() - voting_power::quorum_threshold();
    let mut result = 0;
    while (sum < threshold) {
        let (gas_price, voting_power) = pq.pop_max();
        result = gas_price;
        sum = sum + voting_power;
    };
    result
}

// ==== getter functions ====

public fun total_stake(self: &ValidatorSet): u64 {
    self.total_stake
}

public fun validator_total_stake_amount(self: &ValidatorSet, validator_address: address): u64 {
    let validator = get_validator_ref(&self.active_validators, validator_address);
    validator.total_stake()
}

public fun validator_stake_amount(self: &ValidatorSet, validator_address: address): u64 {
    let validator = get_validator_ref(&self.active_validators, validator_address);
    validator.total_stake()
}

public fun validator_voting_power(self: &ValidatorSet, validator_address: address): u64 {
    let validator = get_validator_ref(&self.active_validators, validator_address);
    validator.voting_power()
}

public fun validator_staking_pool_id(self: &ValidatorSet, validator_address: address): ID {
    let validator = get_validator_ref(&self.active_validators, validator_address);
    validator.staking_pool_id()
}

public fun staking_pool_mappings(self: &ValidatorSet): &Table<ID, address> {
    &self.staking_pool_mappings
}

public fun validator_address_by_pool_id(self: &mut ValidatorSet, pool_id: &ID): address {
    // If the pool id is recorded in the mapping, then it must be either candidate or active.
    if (self.staking_pool_mappings.contains(*pool_id)) {
        self.staking_pool_mappings[*pool_id]
    } else {
        // otherwise it's inactive
        self.inactive_validators[*pool_id].load_validator_maybe_upgrade().sui_address()
    }
}

public(package) fun pool_exchange_rates(
    self: &mut ValidatorSet,
    pool_id: &ID,
): &Table<u64, PoolTokenExchangeRate> {
    // If the pool id is recorded in the mapping, then it must be either candidate or active.
    let validator = if (self.staking_pool_mappings.contains(*pool_id)) {
        let validator_address = self.staking_pool_mappings[*pool_id];
        self.get_active_or_pending_or_candidate_validator_ref(validator_address, ANY_VALIDATOR)
    } else {
        // otherwise it's inactive
        self.inactive_validators[*pool_id].load_validator_maybe_upgrade()
    };

    validator.get_staking_pool_ref().exchange_rates()
}

public(package) fun validator_by_pool_id(self: &mut ValidatorSet, pool_id: &ID): &Validator {
    // If the pool id is recorded in the mapping, then it must be either candidate or active.
    let validator = if (self.staking_pool_mappings.contains(*pool_id)) {
        let validator_address = self.staking_pool_mappings[*pool_id];
        self.get_active_or_pending_or_candidate_validator_ref(validator_address, ANY_VALIDATOR)
    } else {
        // otherwise it's inactive
        self.inactive_validators[*pool_id].load_validator_maybe_upgrade()
    };

    validator
}

/// Get the total number of validators in the next epoch.
public(package) fun next_epoch_validator_count(self: &ValidatorSet): u64 {
    self.active_validators.length() - self.pending_removals.length() + self.pending_active_validators.length()
}

/// Returns true iff the address exists in active validators.
public(package) fun is_active_validator_by_sui_address(
    self: &ValidatorSet,
    validator_address: address,
): bool {
    find_validator(&self.active_validators, validator_address).is_some()
}

// ==== private helpers ====

/// Checks whether `new_validator` is duplicate with any currently active validators.
/// It differs from `is_active_validator_by_sui_address` in that the former checks
/// only the sui address but this function looks at more metadata.
fun is_duplicate_with_active_validator(self: &ValidatorSet, new_validator: &Validator): bool {
    is_duplicate_validator(&self.active_validators, new_validator)
}

public(package) fun is_duplicate_validator(
    validators: &vector<Validator>,
    new_validator: &Validator,
): bool {
    count_duplicates_vec(validators, new_validator) > 0
}

fun count_duplicates_vec(validators: &vector<Validator>, validator: &Validator): u64 {
    validators.count!(|v| v.is_duplicate(validator))
}

/// Checks whether `new_validator` is duplicate with any currently pending validators.
fun is_duplicate_with_pending_validator(self: &ValidatorSet, new_validator: &Validator): bool {
    count_duplicates_tablevec(&self.pending_active_validators, new_validator) > 0
}

fun count_duplicates_tablevec(validators: &TableVec<Validator>, validator: &Validator): u64 {
    let mut result = 0;
    validators.length().do!(|i| {
        if (validators[i].is_duplicate(validator)) {
            result = result + 1;
        };
    });
    result
}

/// Get mutable reference to either a candidate or an active validator by address.
fun get_candidate_or_active_validator_mut(
    self: &mut ValidatorSet,
    validator_address: address,
): &mut Validator {
    if (self.validator_candidates.contains(validator_address)) {
        self.validator_candidates[validator_address].load_validator_maybe_upgrade()
    } else {
        get_validator_mut(&mut self.active_validators, validator_address)
    }
}

/// Find validator by `validator_address`, in `validators`.
/// Returns (true, index) if the validator is found, and the index is its index in the list.
/// If not found, returns (false, 0).
fun find_validator(validators: &vector<Validator>, validator_address: address): Option<u64> {
    validators.find_index!(|v| v.sui_address() == validator_address)
}

/// Find validator by `validator_address`, in `validators`.
/// Returns (true, index) if the validator is found, and the index is its index in the list.
/// If not found, returns (false, 0).
fun find_validator_from_table_vec(
    validators: &TableVec<Validator>,
    validator_address: address,
): Option<u64> {
    let length = validators.length();
    let mut i = 0;
    while (i < length) {
        let v = &validators[i];
        if (v.sui_address() == validator_address) {
            return option::some(i)
        };
        i = i + 1;
    };
    option::none()
}

/// Given a vector of validator addresses, return their indices in the validator set.
/// Aborts if any address isn't in the given validator set.
fun get_validator_indices(
    validators: &vector<Validator>,
    validator_addresses: &vector<address>,
): vector<u64> {
    let mut res = vector[];
    validator_addresses.do_ref!(|addr| {
        let idx = find_validator(validators, *addr).destroy_or!(abort ENotAValidator);
        res.push_back(idx);
    });
    res
}

public(package) fun get_validator_mut(
    validators: &mut vector<Validator>,
    validator_address: address,
): &mut Validator {
    let idx = find_validator(validators, validator_address).destroy_or!(abort ENotAValidator);
    &mut validators[idx]
}

#[test_only]
public(package) fun get_validator_by_address_mut(
    self: &mut ValidatorSet,
    addr: address,
): &mut Validator {
    self.get_candidate_or_active_validator_mut(addr)
}

#[test_only]
public(package) fun get_validator(
    validators: &vector<Validator>,
    validator_address: address,
): &Validator {
    let idx = find_validator(validators, validator_address).destroy_or!(abort ENotAValidator);
    &validators[idx]
}

/// Get mutable reference to an active or (if active does not exist) pending or (if pending and
/// active do not exist) or candidate validator by address.
/// Note: this function should be called carefully, only after verifying the transaction
/// sender has the ability to modify the `Validator`.
fun get_active_or_pending_or_candidate_validator_mut(
    self: &mut ValidatorSet,
    validator_address: address,
    include_candidate: bool,
): &mut Validator {
    let mut validator_index_opt = find_validator(&self.active_validators, validator_address);
    if (validator_index_opt.is_some()) {
        let validator_index = validator_index_opt.extract();
        let validator = &mut self.active_validators[validator_index];
        return validator
    };
    let mut validator_index_opt = find_validator_from_table_vec(
        &self.pending_active_validators,
        validator_address,
    );
    // consider both pending validators and the candidate ones
    if (validator_index_opt.is_some()) {
        let validator_index = validator_index_opt.extract();
        let validator = &mut self.pending_active_validators[validator_index];
        return validator
    };
    assert!(include_candidate, ENotActiveOrPendingValidator);
    self.validator_candidates[validator_address].load_validator_maybe_upgrade()
}

public(package) fun get_validator_mut_with_verified_cap(
    self: &mut ValidatorSet,
    verified_cap: &ValidatorOperationCap,
    include_candidate: bool,
): &mut Validator {
    self.get_active_or_pending_or_candidate_validator_mut(
        *verified_cap.verified_operation_cap_address(),
        include_candidate,
    )
}

public(package) fun get_validator_mut_with_ctx(
    self: &mut ValidatorSet,
    ctx: &TxContext,
): &mut Validator {
    let validator_address = ctx.sender();
    self.get_active_or_pending_or_candidate_validator_mut(validator_address, false)
}

public(package) fun get_validator_mut_with_ctx_including_candidates(
    self: &mut ValidatorSet,
    ctx: &TxContext,
): &mut Validator {
    let validator_address = ctx.sender();
    self.get_active_or_pending_or_candidate_validator_mut(validator_address, true)
}

fun get_validator_ref(validators: &vector<Validator>, validator_address: address): &Validator {
    let idx = find_validator(validators, validator_address).destroy_or!(abort ENotAValidator);
    &validators[idx]
}

public(package) fun get_active_or_pending_or_candidate_validator_ref(
    self: &mut ValidatorSet,
    validator_address: address,
    which_validator: u8,
): &Validator {
    let mut validator_index_opt = find_validator(&self.active_validators, validator_address);
    if (validator_index_opt.is_some() || which_validator == ACTIVE_VALIDATOR_ONLY) {
        let validator_index = validator_index_opt.extract();
        return &self.active_validators[validator_index]
    };
    let mut validator_index_opt = find_validator_from_table_vec(
        &self.pending_active_validators,
        validator_address,
    );
    if (validator_index_opt.is_some() || which_validator == ACTIVE_OR_PENDING_VALIDATOR) {
        let validator_index = validator_index_opt.extract();
        return &self.pending_active_validators[validator_index]
    };
    self.validator_candidates[validator_address].load_validator_maybe_upgrade()
}

public fun get_active_validator_ref(self: &ValidatorSet, addr: address): &Validator {
    let idx = find_validator(&self.active_validators, addr).destroy_or!(abort ENotAValidator);
    &self.active_validators[idx]
}

public fun get_pending_validator_ref(self: &ValidatorSet, addr: address): &Validator {
    let idx = find_validator_from_table_vec(
        &self.pending_active_validators,
        addr,
    ).destroy_or!(abort ENotAPendingValidator);

    &self.pending_active_validators[idx]
}

#[test_only]
public fun get_candidate_validator_ref(
    self: &ValidatorSet,
    validator_address: address,
): &Validator {
    self.validator_candidates[validator_address].get_inner_validator_ref()
}

/// Verify the capability is valid for a Validator.
/// If `active_validator_only` is true, only verify the Cap for an active validator.
/// Otherwise, verify the Cap for au either active or pending validator.
public(package) fun verify_cap(
    self: &mut ValidatorSet,
    cap: &UnverifiedValidatorOperationCap,
    which_validator: u8,
): ValidatorOperationCap {
    let cap_address = *cap.unverified_operation_cap_address();
    let validator = if (which_validator == ACTIVE_VALIDATOR_ONLY) {
        self.get_active_validator_ref(cap_address)
    } else {
        self.get_active_or_pending_or_candidate_validator_ref(cap_address, which_validator)
    };
    assert!(validator.operation_cap_id() == &object::id(cap), EInvalidCap);
    cap.into_verified()
}

/// Process the pending withdraw requests. For each pending request, the validator
/// is removed from `validators` and its staking pool is put into the `inactive_validators` table.
fun process_pending_removals(
    self: &mut ValidatorSet,
    validator_report_records: &mut VecMap<address, VecSet<address>>,
    ctx: &mut TxContext,
) {
    sort_removal_list(&mut self.pending_removals);
    self.pending_removals.length().do!(|_| {
        let index = self.pending_removals.pop_back();
        let validator = self.active_validators.remove(index);
        self.process_validator_departure(
            validator,
            validator_report_records,
            true, // the validator removes itself voluntarily
            ctx,
        );
    });
}

/// Remove `validator` from `self` and return the amount of stake that was removed
fun process_validator_departure(
    self: &mut ValidatorSet,
    mut validator: Validator,
    validator_report_records: &mut VecMap<address, VecSet<address>>,
    is_voluntary: bool,
    ctx: &mut TxContext,
): u64 {
    let new_epoch = ctx.epoch() + 1;
    let validator_address = validator.sui_address();
    let validator_pool_id = validator.staking_pool_id();

    // Remove the validator from our tables.
    self.staking_pool_mappings.remove(validator_pool_id);
    if (self.at_risk_validators.contains(&validator_address)) {
        self.at_risk_validators.remove(&validator_address);
    };

    clean_report_records_leaving_validator(validator_report_records, validator_address);

    event::emit(ValidatorLeaveEvent {
        epoch: new_epoch,
        validator_address,
        staking_pool_id: validator.staking_pool_id(),
        is_voluntary,
    });

    // Deactivate the validator and its staking pool
    let removed_stake = validator.total_stake();
    validator.deactivate(new_epoch);
    self
        .inactive_validators
        .add(
            validator_pool_id,
            validator.wrap_v1(ctx),
        );
    removed_stake
}

fun clean_report_records_leaving_validator(
    validator_report_records: &mut VecMap<address, VecSet<address>>,
    leaving_validator_addr: address,
) {
    // Remove the records about this validator
    if (validator_report_records.contains(&leaving_validator_addr)) {
        validator_report_records.remove(&leaving_validator_addr);
    };

    // Remove the reports submitted by this validator
    let reported_validators = validator_report_records.keys();
    reported_validators.length().do!(|i| {
        let reported_validator_addr = &reported_validators[i];
        let reporters = &mut validator_report_records[reported_validator_addr];
        if (reporters.contains(&leaving_validator_addr)) {
            reporters.remove(&leaving_validator_addr);
            if (reporters.is_empty()) {
                validator_report_records.remove(reported_validator_addr);
            };
        };
    });
}

/// Sort all the pending removal indexes.
fun sort_removal_list(withdraw_list: &mut vector<u64>) {
    let length = withdraw_list.length();
    let mut i = 1;
    while (i < length) {
        let cur = withdraw_list[i];
        let mut j = i;
        while (j > 0) {
            j = j - 1;
            if (withdraw_list[j] > cur) {
                withdraw_list.swap(j, j + 1);
            } else {
                break
            };
        };
        i = i + 1;
    };
}

/// Process all active validators' pending stake deposits and withdraws.
fun process_pending_stakes_and_withdraws(validators: &mut vector<Validator>, ctx: &TxContext) {
    validators.do_mut!(|v| v.process_pending_stakes_and_withdraws(ctx))
}

/// Calculate the total active validator stake.
public(package) fun calculate_total_stakes(validators: &vector<Validator>): u64 {
    let mut stake = 0;
    validators.do_ref!(|v| stake = stake + v.total_stake());
    stake
}

/// Process the pending stake changes for each validator.
fun adjust_stake_and_gas_price(validators: &mut vector<Validator>) {
    validators.do_mut!(|v| v.adjust_stake_and_gas_price())
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
    let mut total_staking_reward_adjustment = 0;
    let mut individual_staking_reward_adjustments = vec_map::empty();
    let mut total_storage_fund_reward_adjustment = 0;
    let mut individual_storage_fund_reward_adjustments = vec_map::empty();

    slashed_validator_indices.destroy!(|validator_index| {
        // Use the slashing rate to compute the amount of staking rewards slashed from this punished validator.
        let unadjusted_staking_reward = unadjusted_staking_reward_amounts[validator_index];
        let staking_reward_adjustment = mul_div!(
            unadjusted_staking_reward,
            reward_slashing_rate,
            BASIS_POINT_DENOMINATOR,
        );

        // Insert into individual mapping and record into the total adjustment sum.
        individual_staking_reward_adjustments.insert(validator_index, staking_reward_adjustment);
        total_staking_reward_adjustment =
            total_staking_reward_adjustment + staking_reward_adjustment;

        // Do the same thing for storage fund rewards.
        let unadjusted_storage_fund_reward = unadjusted_storage_fund_reward_amounts[
            validator_index,
        ];
        let storage_fund_reward_adjustment = mul_div!(
            unadjusted_storage_fund_reward,
            reward_slashing_rate,
            BASIS_POINT_DENOMINATOR,
        );
        individual_storage_fund_reward_adjustments.insert(
            validator_index,
            storage_fund_reward_adjustment,
        );
        total_storage_fund_reward_adjustment =
            total_storage_fund_reward_adjustment + storage_fund_reward_adjustment;
    });

    (
        total_staking_reward_adjustment,
        individual_staking_reward_adjustments,
        total_storage_fund_reward_adjustment,
        individual_storage_fund_reward_adjustments,
    )
}

/// Process the validator report records of the epoch and return the addresses of the
/// non-performant validators according to the input threshold.
fun compute_slashed_validators(
    self: &ValidatorSet,
    mut validator_report_records: VecMap<address, VecSet<address>>,
): vector<address> {
    let mut slashed_validators = vector[];
    while (!validator_report_records.is_empty()) {
        let (validator_address, reporters) = validator_report_records.pop();
        assert!(
            self.is_active_validator_by_sui_address(validator_address),
            ENonValidatorInReportRecords,
        );
        // Sum up the voting power of validators that have reported this validator and check if it has
        // passed the slashing threshold.
        let reporter_votes = sum_voting_power_by_addresses(
            &self.active_validators,
            &reporters.into_keys(),
        );
        if (reporter_votes >= voting_power::quorum_threshold()) {
            slashed_validators.push_back(validator_address);
        }
    };
    slashed_validators
}

/// Given the current list of active validators, the total stake and total reward,
/// calculate the amount of reward each validator should get, without taking into
/// account the tallying rule results.
/// Returns the unadjusted amounts of staking reward and storage fund reward for each validator.
fun compute_unadjusted_reward_distribution(
    validators: &vector<Validator>,
    total_voting_power: u64,
    total_staking_reward: u64,
    total_storage_fund_reward: u64,
): (vector<u64>, vector<u64>) {
    let mut staking_reward_amounts = vector[];
    let mut storage_fund_reward_amounts = vector[];
    let length = validators.length();
    let storage_fund_reward_per_validator = total_storage_fund_reward / length;
    validators.do_ref!(|validator| {
        // Integer divisions will truncate the results. Because of this, we expect that at the end
        // there will be some reward remaining in `total_staking_reward`.
        // Use u128 to avoid multiplication overflow.
        let voting_power = validator.voting_power();
        let reward_amount = mul_div!(voting_power, total_staking_reward, total_voting_power);
        staking_reward_amounts.push_back(reward_amount);
        // Storage fund's share of the rewards are equally distributed among validators.
        storage_fund_reward_amounts.push_back(storage_fund_reward_per_validator);
    });
    (staking_reward_amounts, storage_fund_reward_amounts)
}

/// Use the reward adjustment info to compute the adjusted rewards each validator should get.
/// Returns the staking rewards each validator gets and the storage fund rewards each validator gets.
/// The staking rewards are shared with the stakers while the storage fund ones are not.
fun compute_adjusted_reward_distribution(
    validators: &vector<Validator>,
    total_voting_power: u64,
    total_slashed_validator_voting_power: u64,
    unadjusted_staking_reward_amounts: vector<u64>,
    unadjusted_storage_fund_reward_amounts: vector<u64>,
    total_staking_reward_adjustment: u64,
    individual_staking_reward_adjustments: VecMap<u64, u64>,
    total_storage_fund_reward_adjustment: u64,
    individual_storage_fund_reward_adjustments: VecMap<u64, u64>,
): (vector<u64>, vector<u64>) {
    let total_unslashed_validator_voting_power =
        total_voting_power - total_slashed_validator_voting_power;
    let mut adjusted_staking_reward_amounts = vector[];
    let mut adjusted_storage_fund_reward_amounts = vector[];

    let length = validators.length();
    let num_unslashed_validators = length - individual_staking_reward_adjustments.size();

    length.do!(|i| {
        let validator = &validators[i];
        // Integer divisions will truncate the results. Because of this, we expect that at the end
        // there will be some reward remaining in `total_reward`.
        // Use u128 to avoid multiplication overflow.
        let voting_power = validator.voting_power();

        // Compute adjusted staking reward.
        let unadjusted_staking_reward_amount = unadjusted_staking_reward_amounts[i];
        // If the validator is one of the slashed ones, then subtract the adjustment.
        let adjusted_staking_reward_amount = if (
            individual_staking_reward_adjustments.contains(&i)
        ) {
            let adjustment = individual_staking_reward_adjustments[&i];
            unadjusted_staking_reward_amount - adjustment
        } else {
            // Otherwise the slashed rewards should be distributed among the unslashed
            // validators so add the corresponding adjustment.
            let adjustment = mul_div!(
                total_staking_reward_adjustment,
                voting_power,
                total_unslashed_validator_voting_power,
            );

            unadjusted_staking_reward_amount + adjustment
        };
        adjusted_staking_reward_amounts.push_back(adjusted_staking_reward_amount);

        // Compute adjusted storage fund reward.
        let unadjusted_storage_fund_reward_amount = unadjusted_storage_fund_reward_amounts[i];
        // If the validator is one of the slashed ones, then subtract the adjustment.
        let adjusted_storage_fund_reward_amount = if (
            individual_storage_fund_reward_adjustments.contains(&i)
        ) {
            let adjustment = individual_storage_fund_reward_adjustments[&i];
            unadjusted_storage_fund_reward_amount - adjustment
        } else {
            // Otherwise the slashed rewards should be equally distributed among the unslashed validators.
            let adjustment = total_storage_fund_reward_adjustment / num_unslashed_validators;
            unadjusted_storage_fund_reward_amount + adjustment
        };
        adjusted_storage_fund_reward_amounts.push_back(adjusted_storage_fund_reward_amount);
    });

    (adjusted_staking_reward_amounts, adjusted_storage_fund_reward_amounts)
}

fun distribute_reward(
    validators: &mut vector<Validator>,
    adjusted_staking_reward_amounts: &vector<u64>,
    adjusted_storage_fund_reward_amounts: &vector<u64>,
    staking_rewards: &mut Balance<SUI>,
    storage_fund_reward: &mut Balance<SUI>,
    ctx: &mut TxContext,
) {
    let length = validators.length();
    assert!(length > 0, EValidatorSetEmpty);
    length.do!(|i| {
        let validator = &mut validators[i];
        let staking_reward_amount = adjusted_staking_reward_amounts[i];
        let mut staker_reward = staking_rewards.split(staking_reward_amount);

        // Validator takes a cut of the rewards as commission.
        let validator_commission_amount = mul_div!(
            staking_reward_amount,
            validator.commission_rate(),
            BASIS_POINT_DENOMINATOR,
        );

        // The validator reward = storage_fund_reward + commission.
        let mut validator_reward = staker_reward.split(validator_commission_amount as u64);

        // Add storage fund rewards to the validator's reward.
        validator_reward.join(storage_fund_reward.split(adjusted_storage_fund_reward_amounts[i]));

        // Add rewards to the validator. Don't try and distribute rewards though if the payout is zero.
        if (validator_reward.value() > 0) {
            let validator_address = validator.sui_address();
            let rewards_stake = validator.request_add_stake(
                validator_reward,
                validator_address,
                ctx,
            );
            transfer::public_transfer(rewards_stake, validator_address);
        } else {
            validator_reward.destroy_zero();
        };

        // Add rewards to stake staking pool to auto compound for stakers.
        validator.deposit_stake_rewards(staker_reward);
    });
}

/// Emit events containing information of each validator for the epoch,
/// including stakes, rewards, performance, etc.
fun emit_validator_epoch_events(
    new_epoch: u64,
    vs: &vector<Validator>,
    pool_staking_reward_amounts: &vector<u64>,
    storage_fund_staking_reward_amounts: &vector<u64>,
    report_records: &VecMap<address, VecSet<address>>,
    slashed_validators: &vector<address>,
) {
    let length = vs.length();
    length.do!(|i| {
        let v = &vs[i];
        let validator_address = v.sui_address();
        let tallying_rule_reporters = if (report_records.contains(&validator_address)) {
            report_records[&validator_address].into_keys()
        } else {
            vector[]
        };
        let tallying_rule_global_score = if (slashed_validators.contains(&validator_address)) {
            0
        } else {
            1
        };
        event::emit(ValidatorEpochInfoEventV2 {
            epoch: new_epoch,
            validator_address,
            reference_gas_survey_quote: v.gas_price(),
            stake: v.total_stake(),
            voting_power: v.voting_power(),
            commission_rate: v.commission_rate(),
            pool_staking_reward: pool_staking_reward_amounts[i],
            storage_fund_staking_reward: storage_fund_staking_reward_amounts[i],
            pool_token_exchange_rate: v.pool_token_exchange_rate_at_epoch(new_epoch),
            tallying_rule_reporters,
            tallying_rule_global_score,
        });
    });
}

/// Sum up the total stake of a given list of validator addresses.
public fun sum_voting_power_by_addresses(vs: &vector<Validator>, addresses: &vector<address>): u64 {
    let mut sum = 0;
    addresses.do_ref!(|addr| {
        let validator = get_validator_ref(vs, *addr);
        sum = sum + validator.voting_power();
    });
    sum
}

/// Return the active validators in `self`
public fun active_validators(self: &ValidatorSet): &vector<Validator> {
    &self.active_validators
}

/// Returns true if the `addr` is a validator candidate.
public fun is_validator_candidate(self: &ValidatorSet, addr: address): bool {
    self.validator_candidates.contains(addr)
}

/// Returns true if `addr` is an active validator
public(package) fun is_active_validator(self: &ValidatorSet, addr: address): bool {
    self.active_validators.any!(|v| v.sui_address() == addr)
}

/// Returns true if the staking pool identified by `staking_pool_id` is of an inactive validator.
public fun is_inactive_validator(self: &ValidatorSet, staking_pool_id: ID): bool {
    self.inactive_validators.contains(staking_pool_id)
}

/// Return true if `addr` is currently an at-risk validator below the minimum stake for removal
public(package) fun is_at_risk_validator(self: &ValidatorSet, addr: address): bool {
    self.at_risk_validators.contains(&addr)
}

public(package) fun active_validator_addresses(self: &ValidatorSet): vector<address> {
    let vs = &self.active_validators;
    let mut res = vector[];
    vs.do_ref!(|v| res.push_back(v.sui_address()));
    res
}

macro fun mul_div($a: u64, $b: u64, $c: u64): u64 {
    (($a as u128) * ($b as u128) / ($c as u128)) as u64
}

#[test_only]
public fun find_for_testing(self: &ValidatorSet, validator_address: address): &Validator {
    self.get_candidate_or_active_validator(validator_address)
}

#[test_only]
fun get_candidate_or_active_validator(self: &ValidatorSet, validator_address: address): &Validator {
    if (self.validator_candidates.contains(validator_address)) {
        self.validator_candidates[validator_address].get_inner_validator_ref()
    } else {
        get_validator(&self.active_validators, validator_address)
    }
}
