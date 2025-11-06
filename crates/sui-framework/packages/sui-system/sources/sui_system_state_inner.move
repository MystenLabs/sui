// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui_system::sui_system_state_inner;

use sui::bag::{Self, Bag};
use sui::balance::{Self, Balance};
use sui::coin::Coin;
use sui::event;
use sui::sui::SUI;
use sui::table::Table;
use sui::vec_map::{Self, VecMap};
use sui::vec_set::{Self, VecSet};
use sui_system::stake_subsidy::StakeSubsidy;
use sui_system::staking_pool::{StakedSui, FungibleStakedSui, PoolTokenExchangeRate};
use sui_system::storage_fund::{Self, StorageFund};
use sui_system::validator::{Self, Validator};
use sui_system::validator_cap::{UnverifiedValidatorOperationCap, ValidatorOperationCap};
use sui_system::validator_set::{Self, ValidatorSet};

const ENotValidator: u64 = 0;
const ELimitExceeded: u64 = 1;
#[allow(unused_const)]
const ENotSystemAddress: u64 = 2;
const ECannotReportOneself: u64 = 3;
const EReportRecordNotFound: u64 = 4;
const EBpsTooLarge: u64 = 5;
const ESafeModeGasNotProcessed: u64 = 7;
const EAdvancedToWrongEpoch: u64 = 8;

const BASIS_POINT_DENOMINATOR: u64 = 100_00;

// same as in validator_set
const ACTIVE_VALIDATOR_ONLY: u8 = 1;
const ACTIVE_OR_PENDING_VALIDATOR: u8 = 2;
const ANY_VALIDATOR: u8 = 3;

const SYSTEM_STATE_VERSION_V1: u64 = 1;

const EXTRA_FIELD_EXECUTION_TIME_ESTIMATES_KEY: u64 = 0;
const EXTRA_FIELD_EXECUTION_TIME_ESTIMATES_CHUNK_COUNT_KEY: u64 = 1;

public struct ExecutionTimeObservationChunkKey has copy, drop, store {
    chunk_index: u64,
}

/// A list of system config parameters.
public struct SystemParameters has store {
    /// The duration of an epoch, in milliseconds.
    epoch_duration_ms: u64,
    /// The starting epoch in which stake subsidies start being paid out
    stake_subsidy_start_epoch: u64,
    /// Deprecated.
    /// Maximum number of active validators at any moment.
    /// We do not allow the number of validators in any epoch to go above this.
    max_validator_count: u64,
    /// Deprecated.
    /// Lower-bound on the amount of stake required to become a validator.
    min_validator_joining_stake: u64,
    // Deprecated.
    /// Validators with stake amount below `validator_low_stake_threshold` are considered to
    /// have low stake and will be escorted out of the validator set after being below this
    /// threshold for more than `validator_low_stake_grace_period` number of epochs.
    validator_low_stake_threshold: u64,
    /// Deprecated.
    /// Validators with stake below `validator_very_low_stake_threshold` will be removed
    /// immediately at epoch change, no grace period.
    validator_very_low_stake_threshold: u64,
    /// A validator can have stake below `validator_low_stake_threshold`
    /// for this many epochs before being kicked out.
    validator_low_stake_grace_period: u64,
    /// Any extra fields that's not defined statically.
    extra_fields: Bag,
}

/// Added `min_validator_count`.
public struct SystemParametersV2 has store {
    /// The duration of an epoch, in milliseconds.
    epoch_duration_ms: u64,
    /// The starting epoch in which stake subsidies start being paid out
    stake_subsidy_start_epoch: u64,
    /// Minimum number of active validators at any moment.
    min_validator_count: u64,
    /// Maximum number of active validators at any moment.
    /// We do not allow the number of validators in any epoch to go above this.
    max_validator_count: u64,
    /// Deprecated.
    /// Lower-bound on the amount of stake required to become a validator.
    min_validator_joining_stake: u64,
    /// Deprecated.
    /// Validators with stake amount below `validator_low_stake_threshold` are considered to
    /// have low stake and will be escorted out of the validator set after being below this
    /// threshold for more than `validator_low_stake_grace_period` number of epochs.
    validator_low_stake_threshold: u64,
    /// Deprecated.
    /// Validators with stake below `validator_very_low_stake_threshold` will be removed
    /// immediately at epoch change, no grace period.
    validator_very_low_stake_threshold: u64,
    /// A validator can have stake below `validator_low_stake_threshold`
    /// for this many epochs before being kicked out.
    validator_low_stake_grace_period: u64,
    /// Any extra fields that's not defined statically.
    extra_fields: Bag,
}

/// The top-level object containing all information of the Sui system.
public struct SuiSystemStateInner has store {
    /// The current epoch ID, starting from 0.
    epoch: u64,
    /// The current protocol version, starting from 1.
    protocol_version: u64,
    /// The current version of the system state data structure type.
    /// This is always the same as SuiSystemState.version. Keeping a copy here so that
    /// we know what version it is by inspecting SuiSystemStateInner as well.
    system_state_version: u64,
    /// Contains all information about the validators.
    validators: ValidatorSet,
    /// The storage fund.
    storage_fund: StorageFund,
    /// A list of system config parameters.
    parameters: SystemParameters,
    /// The reference gas price for the current epoch.
    reference_gas_price: u64,
    /// A map storing the records of validator reporting each other.
    /// There is an entry in the map for each validator that has been reported
    /// at least once. The entry VecSet contains all the validators that reported
    /// them. If a validator has never been reported they don't have an entry in this map.
    /// This map persists across epoch: a peer continues being in a reported state until the
    /// reporter doesn't explicitly remove their report.
    /// Note that in case we want to support validator address change in future,
    /// the reports should be based on validator ids
    validator_report_records: VecMap<address, VecSet<address>>,
    /// Schedule of stake subsidies given out each epoch.
    stake_subsidy: StakeSubsidy,
    /// Whether the system is running in a downgraded safe mode due to a non-recoverable bug.
    /// This is set whenever we failed to execute advance_epoch, and ended up executing advance_epoch_safe_mode.
    /// It can be reset once we are able to successfully execute advance_epoch.
    /// The rest of the fields starting with `safe_mode_` are accumulated during safe mode
    /// when advance_epoch_safe_mode is executed. They will eventually be processed once we
    /// are out of safe mode.
    safe_mode: bool,
    safe_mode_storage_rewards: Balance<SUI>,
    safe_mode_computation_rewards: Balance<SUI>,
    safe_mode_storage_rebates: u64,
    safe_mode_non_refundable_storage_fee: u64,
    /// Unix timestamp of the current epoch start
    epoch_start_timestamp_ms: u64,
    /// Any extra fields that's not defined statically.
    extra_fields: Bag,
}

/// Uses SystemParametersV2 as the parameters.
public struct SuiSystemStateInnerV2 has store {
    /// The current epoch ID, starting from 0.
    epoch: u64,
    /// The current protocol version, starting from 1.
    protocol_version: u64,
    /// The current version of the system state data structure type.
    /// This is always the same as SuiSystemState.version. Keeping a copy here so that
    /// we know what version it is by inspecting SuiSystemStateInner as well.
    system_state_version: u64,
    /// Contains all information about the validators.
    validators: ValidatorSet,
    /// The storage fund.
    storage_fund: StorageFund,
    /// A list of system config parameters.
    parameters: SystemParametersV2,
    /// The reference gas price for the current epoch.
    reference_gas_price: u64,
    /// A map storing the records of validator reporting each other.
    /// There is an entry in the map for each validator that has been reported
    /// at least once. The entry VecSet contains all the validators that reported
    /// them. If a validator has never been reported they don't have an entry in this map.
    /// This map persists across epoch: a peer continues being in a reported state until the
    /// reporter doesn't explicitly remove their report.
    /// Note that in case we want to support validator address change in future,
    /// the reports should be based on validator ids
    validator_report_records: VecMap<address, VecSet<address>>,
    /// Schedule of stake subsidies given out each epoch.
    stake_subsidy: StakeSubsidy,
    /// Whether the system is running in a downgraded safe mode due to a non-recoverable bug.
    /// This is set whenever we failed to execute advance_epoch, and ended up executing advance_epoch_safe_mode.
    /// It can be reset once we are able to successfully execute advance_epoch.
    /// The rest of the fields starting with `safe_mode_` are accumulated during safe mode
    /// when advance_epoch_safe_mode is executed. They will eventually be processed once we
    /// are out of safe mode.
    safe_mode: bool,
    safe_mode_storage_rewards: Balance<SUI>,
    safe_mode_computation_rewards: Balance<SUI>,
    safe_mode_storage_rebates: u64,
    safe_mode_non_refundable_storage_fee: u64,
    /// Unix timestamp of the current epoch start
    epoch_start_timestamp_ms: u64,
    /// Any extra fields that's not defined statically.
    extra_fields: Bag,
}

/// Event containing system-level epoch information, emitted during
/// the epoch advancement transaction.
public struct SystemEpochInfoEvent has copy, drop {
    epoch: u64,
    protocol_version: u64,
    reference_gas_price: u64,
    total_stake: u64,
    storage_fund_reinvestment: u64,
    storage_charge: u64,
    storage_rebate: u64,
    storage_fund_balance: u64,
    stake_subsidy_amount: u64,
    total_gas_fees: u64,
    total_stake_rewards_distributed: u64,
    leftover_storage_fund_inflow: u64,
}

// ==== functions that can only be called by genesis ====

/// Create a new SuiSystemState object and make it shared.
/// This function will be called only once in genesis.
public(package) fun create(
    validators: vector<Validator>,
    initial_storage_fund: Balance<SUI>,
    protocol_version: u64,
    epoch_start_timestamp_ms: u64,
    parameters: SystemParameters,
    stake_subsidy: StakeSubsidy,
    ctx: &mut TxContext,
): SuiSystemStateInner {
    let validators = validator_set::new(validators, ctx);
    let reference_gas_price = validators.derive_reference_gas_price();
    // This type is fixed as it's created at genesis. It should not be updated during type upgrade.
    let system_state = SuiSystemStateInner {
        epoch: 0,
        protocol_version,
        system_state_version: genesis_system_state_version(),
        validators,
        storage_fund: storage_fund::new(initial_storage_fund),
        parameters,
        reference_gas_price,
        validator_report_records: vec_map::empty(),
        stake_subsidy,
        safe_mode: false,
        safe_mode_storage_rewards: balance::zero(),
        safe_mode_computation_rewards: balance::zero(),
        safe_mode_storage_rebates: 0,
        safe_mode_non_refundable_storage_fee: 0,
        epoch_start_timestamp_ms,
        extra_fields: bag::new(ctx),
    };
    system_state
}

public(package) fun create_system_parameters(
    epoch_duration_ms: u64,
    stake_subsidy_start_epoch: u64,
    // Validator committee parameters
    max_validator_count: u64,
    min_validator_joining_stake: u64,
    validator_low_stake_threshold: u64,
    validator_very_low_stake_threshold: u64,
    validator_low_stake_grace_period: u64,
    ctx: &mut TxContext,
): SystemParameters {
    SystemParameters {
        epoch_duration_ms,
        stake_subsidy_start_epoch,
        max_validator_count,
        min_validator_joining_stake,
        validator_low_stake_threshold,
        validator_very_low_stake_threshold,
        validator_low_stake_grace_period,
        extra_fields: bag::new(ctx),
    }
}

public(package) fun v1_to_v2(self: SuiSystemStateInner): SuiSystemStateInnerV2 {
    let SuiSystemStateInner {
        epoch,
        protocol_version,
        system_state_version: _,
        validators,
        storage_fund,
        parameters,
        reference_gas_price,
        validator_report_records,
        stake_subsidy,
        safe_mode,
        safe_mode_storage_rewards,
        safe_mode_computation_rewards,
        safe_mode_storage_rebates,
        safe_mode_non_refundable_storage_fee,
        epoch_start_timestamp_ms,
        extra_fields: state_extra_fields,
    } = self;
    let SystemParameters {
        epoch_duration_ms,
        stake_subsidy_start_epoch,
        max_validator_count,
        min_validator_joining_stake,
        validator_low_stake_threshold,
        validator_very_low_stake_threshold,
        validator_low_stake_grace_period,
        extra_fields: param_extra_fields,
    } = parameters;
    SuiSystemStateInnerV2 {
        epoch,
        protocol_version,
        system_state_version: 2,
        validators,
        storage_fund,
        parameters: SystemParametersV2 {
            epoch_duration_ms,
            stake_subsidy_start_epoch,
            min_validator_count: 4,
            max_validator_count,
            min_validator_joining_stake,
            validator_low_stake_threshold,
            validator_very_low_stake_threshold,
            validator_low_stake_grace_period,
            extra_fields: param_extra_fields,
        },
        reference_gas_price,
        validator_report_records,
        stake_subsidy,
        safe_mode,
        safe_mode_storage_rewards,
        safe_mode_computation_rewards,
        safe_mode_storage_rebates,
        safe_mode_non_refundable_storage_fee,
        epoch_start_timestamp_ms,
        extra_fields: state_extra_fields,
    }
}

// ==== public(package) functions ====

/// Can be called by anyone who wishes to become a validator candidate and starts accruing delegated
/// stakes in their staking pool. Once they have at least `MIN_VALIDATOR_JOINING_STAKE` amount of stake they
/// can call `request_add_validator` to officially become an active validator at the next epoch.
/// Aborts if the caller is already a pending or active validator, or a validator candidate.
/// Note: `proof_of_possession` MUST be a valid signature using sui_address and protocol_pubkey_bytes.
/// To produce a valid PoP, run [fn test_proof_of_possession].
public(package) fun request_add_validator_candidate(
    self: &mut SuiSystemStateInnerV2,
    pubkey_bytes: vector<u8>,
    network_pubkey_bytes: vector<u8>,
    worker_pubkey_bytes: vector<u8>,
    proof_of_possession: vector<u8>,
    name: vector<u8>,
    description: vector<u8>,
    image_url: vector<u8>,
    project_url: vector<u8>,
    net_address: vector<u8>,
    p2p_address: vector<u8>,
    primary_address: vector<u8>,
    worker_address: vector<u8>,
    gas_price: u64,
    commission_rate: u64,
    ctx: &mut TxContext,
) {
    let validator = validator::new(
        ctx.sender(),
        pubkey_bytes,
        network_pubkey_bytes,
        worker_pubkey_bytes,
        proof_of_possession,
        name,
        description,
        image_url,
        project_url,
        net_address,
        p2p_address,
        primary_address,
        worker_address,
        gas_price,
        commission_rate,
        ctx,
    );

    self.validators.request_add_validator_candidate(validator, ctx);
}

/// Called by a validator candidate to remove themselves from the candidacy. After this call
/// their staking pool becomes deactivate.
public(package) fun request_remove_validator_candidate(
    self: &mut SuiSystemStateInnerV2,
    ctx: &mut TxContext,
) {
    self.validators.request_remove_validator_candidate(ctx);
}

/// Called by a validator candidate to add themselves to the active validator set beginning next epoch.
/// Aborts if the validator is a duplicate with one of the pending or active validators, or if the amount of
/// stake the validator has doesn't meet the min threshold, or if the number of new validators for the next
/// epoch has already reached the maximum.
public(package) fun request_add_validator(self: &mut SuiSystemStateInnerV2, ctx: &TxContext) {
    assert!(
        self.validators.next_epoch_validator_count() < self.parameters.max_validator_count,
        ELimitExceeded,
    );

    self.validators.request_add_validator(ctx);
}

/// A validator can call this function to request a removal in the next epoch.
/// We use the sender of `ctx` to look up the validator
/// (i.e. sender must match the sui_address in the validator).
/// At the end of the epoch, the `validator` object will be returned to the sui_address
/// of the validator.
public(package) fun request_remove_validator(self: &mut SuiSystemStateInnerV2, ctx: &TxContext) {
    // Only check min validator condition if the current number of validators satisfy the constraint.
    // This is so that if we somehow already are in a state where we have less than min validators, it no longer matters
    // and is ok to stay so. This is useful for a test setup.
    if (self.validators.active_validators().length() >= self.parameters.min_validator_count) {
        assert!(
            self.validators.next_epoch_validator_count() > self.parameters.min_validator_count,
            ELimitExceeded,
        );
    };

    self.validators.request_remove_validator(ctx)
}

/// A validator can call this function to submit a new gas price quote, to be
/// used for the reference gas price calculation at the end of the epoch.
public(package) fun request_set_gas_price(
    self: &mut SuiSystemStateInnerV2,
    cap: &UnverifiedValidatorOperationCap,
    new_gas_price: u64,
) {
    // Verify the represented address is an active or pending validator, and the capability is still valid.
    let verified_cap = self.validators.verify_cap(cap, ACTIVE_OR_PENDING_VALIDATOR);
    let validator = self
        .validators
        .get_validator_mut_with_verified_cap(&verified_cap, false /* include_candidate */);

    validator.request_set_gas_price(verified_cap, new_gas_price);
}

/// This function is used to set new gas price for candidate validators
public(package) fun set_candidate_validator_gas_price(
    self: &mut SuiSystemStateInnerV2,
    cap: &UnverifiedValidatorOperationCap,
    new_gas_price: u64,
) {
    // Verify the represented address is an active or pending validator, and the capability is still valid.
    let verified_cap = self.validators.verify_cap(cap, ANY_VALIDATOR);
    let candidate = self
        .validators
        .get_validator_mut_with_verified_cap(&verified_cap, true /* include_candidate */);
    candidate.set_candidate_gas_price(verified_cap, new_gas_price)
}

/// A validator can call this function to set a new commission rate, updated at the end of
/// the epoch.
public(package) fun request_set_commission_rate(
    self: &mut SuiSystemStateInnerV2,
    new_commission_rate: u64,
    ctx: &TxContext,
) {
    self
        .validators
        .request_set_commission_rate(
            new_commission_rate,
            ctx,
        )
}

/// This function is used to set new commission rate for candidate validators
public(package) fun set_candidate_validator_commission_rate(
    self: &mut SuiSystemStateInnerV2,
    new_commission_rate: u64,
    ctx: &TxContext,
) {
    let candidate = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    candidate.set_candidate_commission_rate(new_commission_rate)
}

/// Add stake to a validator's staking pool.
public(package) fun request_add_stake(
    self: &mut SuiSystemStateInnerV2,
    stake: Coin<SUI>,
    validator_address: address,
    ctx: &mut TxContext,
): StakedSui {
    self
        .validators
        .request_add_stake(
            validator_address,
            stake.into_balance(),
            ctx,
        )
}

/// Add stake to a validator's staking pool using multiple coins.
public(package) fun request_add_stake_mul_coin(
    self: &mut SuiSystemStateInnerV2,
    stakes: vector<Coin<SUI>>,
    stake_amount: Option<u64>,
    validator_address: address,
    ctx: &mut TxContext,
): StakedSui {
    let balance = extract_coin_balance(stakes, stake_amount, ctx);
    self.validators.request_add_stake(validator_address, balance, ctx)
}

/// Withdraw some portion of a stake from a validator's staking pool.
public(package) fun request_withdraw_stake(
    self: &mut SuiSystemStateInnerV2,
    staked_sui: StakedSui,
    ctx: &TxContext,
): Balance<SUI> {
    self.validators.request_withdraw_stake(staked_sui, ctx)
}

public(package) fun convert_to_fungible_staked_sui(
    self: &mut SuiSystemStateInnerV2,
    staked_sui: StakedSui,
    ctx: &mut TxContext,
): FungibleStakedSui {
    self.validators.convert_to_fungible_staked_sui(staked_sui, ctx)
}

public(package) fun redeem_fungible_staked_sui(
    self: &mut SuiSystemStateInnerV2,
    fungible_staked_sui: FungibleStakedSui,
    ctx: &TxContext,
): Balance<SUI> {
    self.validators.redeem_fungible_staked_sui(fungible_staked_sui, ctx)
}

/// Report a validator as a bad or non-performant actor in the system.
/// Succeeds if all the following are satisfied:
/// 1. both the reporter in `cap` and the input `reportee_addr` are active validators.
/// 2. reporter and reportee not the same address.
/// 3. the cap object is still valid.
/// This function is idempotent.
public(package) fun report_validator(
    self: &mut SuiSystemStateInnerV2,
    cap: &UnverifiedValidatorOperationCap,
    reportee_addr: address,
) {
    // Reportee needs to be an active validator
    assert!(self.validators.is_active_validator_by_sui_address(reportee_addr), ENotValidator);
    // Verify the represented reporter address is an active validator, and the capability is still valid.
    let verified_cap = self.validators.verify_cap(cap, ACTIVE_VALIDATOR_ONLY);
    report_validator_impl(verified_cap, reportee_addr, &mut self.validator_report_records);
}

/// Undo a `report_validator` action. Aborts if
/// 1. the reportee is not a currently active validator or
/// 2. the sender has not previously reported the `reportee_addr`, or
/// 3. the cap is not valid
public(package) fun undo_report_validator(
    self: &mut SuiSystemStateInnerV2,
    cap: &UnverifiedValidatorOperationCap,
    reportee_addr: address,
) {
    let verified_cap = self.validators.verify_cap(cap, ACTIVE_VALIDATOR_ONLY);
    undo_report_validator_impl(verified_cap, reportee_addr, &mut self.validator_report_records);
}

fun report_validator_impl(
    verified_cap: ValidatorOperationCap,
    reportee_addr: address,
    validator_report_records: &mut VecMap<address, VecSet<address>>,
) {
    let reporter_address = *verified_cap.verified_operation_cap_address();
    assert!(reporter_address != reportee_addr, ECannotReportOneself);
    if (!validator_report_records.contains(&reportee_addr)) {
        validator_report_records.insert(reportee_addr, vec_set::singleton(reporter_address));
    } else {
        let reporters = &mut validator_report_records[&reportee_addr];
        if (!reporters.contains(&reporter_address)) {
            reporters.insert(reporter_address);
        }
    }
}

fun undo_report_validator_impl(
    verified_cap: ValidatorOperationCap,
    reportee_addr: address,
    validator_report_records: &mut VecMap<address, VecSet<address>>,
) {
    assert!(validator_report_records.contains(&reportee_addr), EReportRecordNotFound);
    let reporters = &mut validator_report_records[&reportee_addr];

    let reporter_addr = *verified_cap.verified_operation_cap_address();
    assert!(reporters.contains(&reporter_addr), EReportRecordNotFound);

    reporters.remove(&reporter_addr);
    if (reporters.is_empty()) {
        validator_report_records.remove(&reportee_addr);
    }
}

// ==== validator metadata management functions ====

/// Create a new `UnverifiedValidatorOperationCap`, transfer it to the
/// validator and registers it. The original object is thus revoked.
public(package) fun rotate_operation_cap(self: &mut SuiSystemStateInnerV2, ctx: &mut TxContext) {
    let validator = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    validator.new_unverified_validator_operation_cap_and_transfer(ctx);
}

/// Update a validator's name.
public(package) fun update_validator_name(
    self: &mut SuiSystemStateInnerV2,
    name: vector<u8>,
    ctx: &TxContext,
) {
    let validator = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    validator.update_name(name);
}

/// Update a validator's description
public(package) fun update_validator_description(
    self: &mut SuiSystemStateInnerV2,
    description: vector<u8>,
    ctx: &TxContext,
) {
    let validator = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    validator.update_description(description);
}

/// Update a validator's image url
public(package) fun update_validator_image_url(
    self: &mut SuiSystemStateInnerV2,
    image_url: vector<u8>,
    ctx: &TxContext,
) {
    let validator = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    validator.update_image_url(image_url);
}

/// Update a validator's project url
public(package) fun update_validator_project_url(
    self: &mut SuiSystemStateInnerV2,
    project_url: vector<u8>,
    ctx: &TxContext,
) {
    let validator = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    validator.update_project_url(project_url);
}

/// Update a validator's network address.
/// The change will only take effects starting from the next epoch.
public(package) fun update_validator_next_epoch_network_address(
    self: &mut SuiSystemStateInnerV2,
    network_address: vector<u8>,
    ctx: &TxContext,
) {
    let validator = self.validators.get_validator_mut_with_ctx(ctx);
    validator.update_next_epoch_network_address(network_address);
    let validator: &Validator = validator; // Avoid parallel mutable borrow.
    self.validators.assert_no_pending_or_active_duplicates(validator);
}

/// Update candidate validator's network address.
public(package) fun update_candidate_validator_network_address(
    self: &mut SuiSystemStateInnerV2,
    network_address: vector<u8>,
    ctx: &TxContext,
) {
    let candidate = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    candidate.update_candidate_network_address(network_address);
}

/// Update a validator's p2p address.
/// The change will only take effects starting from the next epoch.
public(package) fun update_validator_next_epoch_p2p_address(
    self: &mut SuiSystemStateInnerV2,
    p2p_address: vector<u8>,
    ctx: &TxContext,
) {
    let validator = self.validators.get_validator_mut_with_ctx(ctx);
    validator.update_next_epoch_p2p_address(p2p_address);
    let validator: &Validator = validator; // Avoid parallel mutable borrow.
    self.validators.assert_no_pending_or_active_duplicates(validator);
}

/// Update candidate validator's p2p address.
public(package) fun update_candidate_validator_p2p_address(
    self: &mut SuiSystemStateInnerV2,
    p2p_address: vector<u8>,
    ctx: &TxContext,
) {
    let candidate = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    candidate.update_candidate_p2p_address(p2p_address);
}

/// Update a validator's narwhal primary address.
/// The change will only take effects starting from the next epoch.
public(package) fun update_validator_next_epoch_primary_address(
    self: &mut SuiSystemStateInnerV2,
    primary_address: vector<u8>,
    ctx: &TxContext,
) {
    let validator = self.validators.get_validator_mut_with_ctx(ctx);
    validator.update_next_epoch_primary_address(primary_address);
}

/// Update candidate validator's narwhal primary address.
public(package) fun update_candidate_validator_primary_address(
    self: &mut SuiSystemStateInnerV2,
    primary_address: vector<u8>,
    ctx: &TxContext,
) {
    let candidate = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    candidate.update_candidate_primary_address(primary_address);
}

/// Update a validator's narwhal worker address.
/// The change will only take effects starting from the next epoch.
public(package) fun update_validator_next_epoch_worker_address(
    self: &mut SuiSystemStateInnerV2,
    worker_address: vector<u8>,
    ctx: &TxContext,
) {
    let validator = self.validators.get_validator_mut_with_ctx(ctx);
    validator.update_next_epoch_worker_address(worker_address);
}

/// Update candidate validator's narwhal worker address.
public(package) fun update_candidate_validator_worker_address(
    self: &mut SuiSystemStateInnerV2,
    worker_address: vector<u8>,
    ctx: &TxContext,
) {
    let candidate = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    candidate.update_candidate_worker_address(worker_address);
}

/// Update a validator's public key of protocol key and proof of possession.
/// The change will only take effects starting from the next epoch.
public(package) fun update_validator_next_epoch_protocol_pubkey(
    self: &mut SuiSystemStateInnerV2,
    protocol_pubkey: vector<u8>,
    proof_of_possession: vector<u8>,
    ctx: &TxContext,
) {
    let validator = self.validators.get_validator_mut_with_ctx(ctx);
    validator.update_next_epoch_protocol_pubkey(protocol_pubkey, proof_of_possession);
    let validator: &Validator = validator; // Avoid parallel mutable borrow.
    self.validators.assert_no_pending_or_active_duplicates(validator);
}

/// Update candidate validator's public key of protocol key and proof of possession.
public(package) fun update_candidate_validator_protocol_pubkey(
    self: &mut SuiSystemStateInnerV2,
    protocol_pubkey: vector<u8>,
    proof_of_possession: vector<u8>,
    ctx: &TxContext,
) {
    let candidate = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    candidate.update_candidate_protocol_pubkey(protocol_pubkey, proof_of_possession);
}

/// Update a validator's public key of worker key.
/// The change will only take effects starting from the next epoch.
public(package) fun update_validator_next_epoch_worker_pubkey(
    self: &mut SuiSystemStateInnerV2,
    worker_pubkey: vector<u8>,
    ctx: &TxContext,
) {
    let validator = self.validators.get_validator_mut_with_ctx(ctx);
    validator.update_next_epoch_worker_pubkey(worker_pubkey);
    let validator: &Validator = validator; // Avoid parallel mutable borrow.
    self.validators.assert_no_pending_or_active_duplicates(validator);
}

/// Update candidate validator's public key of worker key.
public(package) fun update_candidate_validator_worker_pubkey(
    self: &mut SuiSystemStateInnerV2,
    worker_pubkey: vector<u8>,
    ctx: &TxContext,
) {
    let candidate = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    candidate.update_candidate_worker_pubkey(worker_pubkey);
}

/// Update a validator's public key of network key.
/// The change will only take effects starting from the next epoch.
public(package) fun update_validator_next_epoch_network_pubkey(
    self: &mut SuiSystemStateInnerV2,
    network_pubkey: vector<u8>,
    ctx: &TxContext,
) {
    let validator = self.validators.get_validator_mut_with_ctx(ctx);
    validator.update_next_epoch_network_pubkey(network_pubkey);
    let validator: &Validator = validator; // Avoid parallel mutable borrow.
    self.validators.assert_no_pending_or_active_duplicates(validator);
}

/// Update candidate validator's public key of network key.
public(package) fun update_candidate_validator_network_pubkey(
    self: &mut SuiSystemStateInnerV2,
    network_pubkey: vector<u8>,
    ctx: &TxContext,
) {
    let candidate = self.validators.get_validator_mut_with_ctx_including_candidates(ctx);
    candidate.update_candidate_network_pubkey(network_pubkey);
}

/// This function should be called at the end of an epoch, and advances the system to the next epoch.
/// It does the following things:
/// 1. Add storage charge to the storage fund.
/// 2. Burn the storage rebates from the storage fund. These are already refunded to transaction sender's
///    gas coins.
/// 3. Distribute computation charge to validator stake.
/// 4. Update all validators.
public(package) fun advance_epoch(
    self: &mut SuiSystemStateInnerV2,
    new_epoch: u64,
    next_protocol_version: u64,
    mut storage_reward: Balance<SUI>,
    mut computation_reward: Balance<SUI>,
    mut storage_rebate_amount: u64,
    mut non_refundable_storage_fee_amount: u64,
    // share of storage fund's rewards that's reinvested
    // into storage fund, in basis point.
    storage_fund_reinvest_rate: u64,
    reward_slashing_rate: u64, // how much rewards are slashed to punish a validator, in bps.
    epoch_start_timestamp_ms: u64, // Timestamp of the epoch start
    ctx: &mut TxContext,
): Balance<SUI> {
    let prev_epoch_start_timestamp = self.epoch_start_timestamp_ms;
    self.epoch_start_timestamp_ms = epoch_start_timestamp_ms;

    let bps_denominator = BASIS_POINT_DENOMINATOR;
    // Rates can't be higher than 100%.
    assert!(
        storage_fund_reinvest_rate <= bps_denominator
        && reward_slashing_rate <= bps_denominator,
        EBpsTooLarge,
    );

    // TODO: remove this in later upgrade.
    if (self.parameters.stake_subsidy_start_epoch > 0) {
        self.parameters.stake_subsidy_start_epoch = 20;
    };

    // Accumulate the gas summary during safe_mode before processing any rewards:
    let safe_mode_storage_rewards = self.safe_mode_storage_rewards.withdraw_all();
    storage_reward.join(safe_mode_storage_rewards);
    let safe_mode_computation_rewards = self.safe_mode_computation_rewards.withdraw_all();
    computation_reward.join(safe_mode_computation_rewards);
    storage_rebate_amount = storage_rebate_amount + self.safe_mode_storage_rebates;
    self.safe_mode_storage_rebates = 0;
    non_refundable_storage_fee_amount =
        non_refundable_storage_fee_amount + self.safe_mode_non_refundable_storage_fee;
    self.safe_mode_non_refundable_storage_fee = 0;

    let total_validators_stake = self.validators.total_stake();
    let storage_fund_balance = self.storage_fund.total_balance();
    let total_stake = storage_fund_balance + total_validators_stake;

    let storage_charge = storage_reward.value();
    let computation_charge = computation_reward.value();
    let mut stake_subsidy = balance::zero();

    // during the transition from epoch N to epoch N + 1, ctx.epoch() will return N
    let old_epoch = ctx.epoch();
    // Include stake subsidy in the rewards given out to validators and stakers.
    // Delay distributing any stake subsidies until after `stake_subsidy_start_epoch`.
    // And if this epoch is shorter than the regular epoch duration, don't distribute any stake subsidy.
    if (
        old_epoch >= self.parameters.stake_subsidy_start_epoch  &&
        epoch_start_timestamp_ms >= prev_epoch_start_timestamp + self.parameters.epoch_duration_ms
    ) {
        // special case for epoch 560 -> 561 change bug. add extra subsidies for "safe mode"
        // where reward distribution was skipped. use distribution counter and epoch check to
        // avoiding affecting devnet and testnet
        if (self.stake_subsidy.get_distribution_counter() == 540 && old_epoch > 560) {
            // safe mode was entered on the change from 560 to 561. so 560 was the first epoch without proper subsidy distribution
            let first_safe_mode_epoch = 560;
            let safe_mode_epoch_count = old_epoch - first_safe_mode_epoch;
            safe_mode_epoch_count.do!(|_| {
                stake_subsidy.join(self.stake_subsidy.advance_epoch());
            });
            // done with catchup for safe mode epochs. distribution counter is now >540, we won't hit this again
            // fall through to the normal logic, which will add subsidies for the current epoch
        };
        stake_subsidy.join(self.stake_subsidy.advance_epoch());
    };

    let stake_subsidy_amount = stake_subsidy.value();
    computation_reward.join(stake_subsidy);

    let storage_fund_reward_amount = mul_div!(
        storage_fund_balance,
        computation_charge,
        total_stake,
    );
    let mut storage_fund_reward = computation_reward.split(storage_fund_reward_amount as u64);
    let storage_fund_reinvestment_amount = mul_div!(
        storage_fund_reward_amount,
        storage_fund_reinvest_rate,
        BASIS_POINT_DENOMINATOR,
    );
    let storage_fund_reinvestment = storage_fund_reward.split(
        storage_fund_reinvestment_amount,
    );

    self.epoch = self.epoch + 1;
    // Sanity check to make sure we are advancing to the right epoch.
    assert!(new_epoch == self.epoch, EAdvancedToWrongEpoch);

    let computation_reward_amount_before_distribution = computation_reward.value();
    let storage_fund_reward_amount_before_distribution = storage_fund_reward.value();

    self
        .validators
        .advance_epoch(
            &mut computation_reward,
            &mut storage_fund_reward,
            &mut self.validator_report_records,
            reward_slashing_rate,
            self.parameters.validator_low_stake_grace_period,
            ctx,
        );

    let new_total_stake = self.validators.total_stake();

    let computation_reward_amount_after_distribution = computation_reward.value();
    let storage_fund_reward_amount_after_distribution = storage_fund_reward.value();
    let computation_reward_distributed =
        computation_reward_amount_before_distribution - computation_reward_amount_after_distribution;
    let storage_fund_reward_distributed =
        storage_fund_reward_amount_before_distribution - storage_fund_reward_amount_after_distribution;

    self.protocol_version = next_protocol_version;

    // Derive the reference gas price for the new epoch
    self.reference_gas_price = self.validators.derive_reference_gas_price();
    // Because of precision issues with integer divisions, we expect that there will be some
    // remaining balance in `storage_fund_reward` and `computation_reward`.
    // All of these go to the storage fund.
    let mut leftover_staking_rewards = storage_fund_reward;
    leftover_staking_rewards.join(computation_reward);
    let leftover_storage_fund_inflow = leftover_staking_rewards.value();

    let refunded_storage_rebate = self
        .storage_fund
        .advance_epoch(
            storage_reward,
            storage_fund_reinvestment,
            leftover_staking_rewards,
            storage_rebate_amount,
            non_refundable_storage_fee_amount,
        );

    event::emit(SystemEpochInfoEvent {
        epoch: self.epoch,
        protocol_version: self.protocol_version,
        reference_gas_price: self.reference_gas_price,
        total_stake: new_total_stake,
        storage_charge,
        storage_fund_reinvestment: storage_fund_reinvestment_amount as u64,
        storage_rebate: storage_rebate_amount,
        storage_fund_balance: self.storage_fund.total_balance(),
        stake_subsidy_amount,
        total_gas_fees: computation_charge,
        total_stake_rewards_distributed: computation_reward_distributed + storage_fund_reward_distributed,
        leftover_storage_fund_inflow,
    });
    self.safe_mode = false;
    // Double check that the gas from safe mode has been processed.
    assert!(
        self.safe_mode_storage_rebates == 0
        && self.safe_mode_storage_rewards.value() == 0
        && self.safe_mode_computation_rewards.value() == 0,
        ESafeModeGasNotProcessed,
    );

    // Return the storage rebate split from storage fund that's already refunded to the transaction senders.
    // This will be burnt at the last step of epoch change programmable transaction.
    refunded_storage_rebate
}

/// Return the current epoch number. Useful for applications that need a coarse-grained concept of time,
/// since epochs are ever-increasing and epoch changes are intended to happen every 24 hours.
public(package) fun epoch(self: &SuiSystemStateInnerV2): u64 {
    self.epoch
}

public(package) fun protocol_version(self: &SuiSystemStateInnerV2): u64 {
    self.protocol_version
}

public(package) fun system_state_version(self: &SuiSystemStateInnerV2): u64 {
    self.system_state_version
}

/// This function always return the genesis system state version, which is used to create the system state in genesis.
/// It should never change for a given network.
public(package) fun genesis_system_state_version(): u64 {
    SYSTEM_STATE_VERSION_V1
}

/// Returns unix timestamp of the start of current epoch
public(package) fun epoch_start_timestamp_ms(self: &SuiSystemStateInnerV2): u64 {
    self.epoch_start_timestamp_ms
}

/// Returns the total amount staked with `validator_addr`.
/// Aborts if `validator_addr` is not an active validator.
public(package) fun validator_stake_amount(
    self: &SuiSystemStateInnerV2,
    validator_addr: address,
): u64 {
    self.validators.validator_total_stake_amount(validator_addr)
}

/// Returns the voting power for `validator_addr`.
/// Aborts if `validator_addr` is not an active validator.
public(package) fun active_validator_voting_powers(
    self: &SuiSystemStateInnerV2,
): VecMap<address, u64> {
    let active_validators = self.active_validator_addresses();
    let mut voting_powers = vec_map::empty();
    active_validators.destroy!(|validator| {
        let voting_power = self.validators.validator_voting_power(validator);
        voting_powers.insert(validator, voting_power);
    });
    voting_powers
}

/// Returns the staking pool id of a given validator.
/// Aborts if `validator_addr` is not an active validator.
public(package) fun validator_staking_pool_id(
    self: &SuiSystemStateInnerV2,
    validator_addr: address,
): ID {
    self.validators.validator_staking_pool_id(validator_addr)
}

/// Returns reference to the staking pool mappings that map pool ids to active validator addresses
public(package) fun validator_staking_pool_mappings(
    self: &SuiSystemStateInnerV2,
): &Table<ID, address> {
    self.validators.staking_pool_mappings()
}

/// Returns all the validators who are currently reporting `addr`
public(package) fun get_reporters_of(self: &SuiSystemStateInnerV2, addr: address): VecSet<address> {
    if (self.validator_report_records.contains(&addr)) self.validator_report_records[&addr]
    else vec_set::empty()
}

public(package) fun get_storage_fund_total_balance(self: &SuiSystemStateInnerV2): u64 {
    self.storage_fund.total_balance()
}

public(package) fun get_storage_fund_object_rebates(self: &SuiSystemStateInnerV2): u64 {
    self.storage_fund.total_object_storage_rebates()
}

public(package) fun validator_address_by_pool_id(
    self: &mut SuiSystemStateInnerV2,
    pool_id: &ID,
): address {
    self.validators.validator_address_by_pool_id(pool_id)
}

public(package) fun pool_exchange_rates(
    self: &mut SuiSystemStateInnerV2,
    pool_id: &ID,
): &Table<u64, PoolTokenExchangeRate> {
    self.validators.pool_exchange_rates(pool_id)
}

public(package) fun active_validator_addresses(self: &SuiSystemStateInnerV2): vector<address> {
    self.validators.active_validator_addresses()
}

#[allow(lint(self_transfer))]
/// Extract required Balance from vector of Coin<SUI>, transfer the remainder back to sender.
fun extract_coin_balance(
    mut coins: vector<Coin<SUI>>,
    amount: Option<u64>,
    ctx: &mut TxContext,
): Balance<SUI> {
    let acc = coins.pop_back();
    let merged = coins.fold!(acc, |mut acc, coin| { acc.join(coin); acc });
    let mut total_balance = merged.into_balance();
    // return the full amount if amount is not specified
    if (amount.is_some()) {
        let amount = amount.destroy_some();
        let balance = total_balance.split(amount);
        // transfer back the remainder if non zero.
        if (total_balance.value() > 0) {
            transfer::public_transfer(total_balance.into_coin(ctx), ctx.sender());
        } else {
            total_balance.destroy_zero();
        };
        balance
    } else {
        total_balance
    }
}

public(package) fun store_execution_time_estimates(
    self: &mut SuiSystemStateInnerV2,
    estimates: vector<u8>,
) {
    if (self.extra_fields.contains(EXTRA_FIELD_EXECUTION_TIME_ESTIMATES_KEY)) {
        self.extra_fields.remove<_, vector<u8>>(EXTRA_FIELD_EXECUTION_TIME_ESTIMATES_KEY);
    };
    self.extra_fields.add(EXTRA_FIELD_EXECUTION_TIME_ESTIMATES_KEY, estimates);
}

public(package) fun store_execution_time_estimates_v2(
    self: &mut SuiSystemStateInnerV2,
    estimate_chunks: vector<vector<u8>>,
) {
    if (self.extra_fields.contains(EXTRA_FIELD_EXECUTION_TIME_ESTIMATES_KEY)) {
        self.extra_fields.remove<_, vector<u8>>(EXTRA_FIELD_EXECUTION_TIME_ESTIMATES_KEY);
    };

    if (self.extra_fields.contains(EXTRA_FIELD_EXECUTION_TIME_ESTIMATES_CHUNK_COUNT_KEY)) {
        let existing_chunk_count: u64 = self
            .extra_fields
            .remove<_, u64>(EXTRA_FIELD_EXECUTION_TIME_ESTIMATES_CHUNK_COUNT_KEY);

        let mut chunk_idx = 0;
        while (chunk_idx < existing_chunk_count) {
            let chunk_key = ExecutionTimeObservationChunkKey { chunk_index: chunk_idx };
            if (self.extra_fields.contains(chunk_key)) {
                self.extra_fields.remove<ExecutionTimeObservationChunkKey, vector<u8>>(chunk_key);
            };
            chunk_idx = chunk_idx + 1;
        };
    };

    let total_chunks = estimate_chunks.length();
    if (total_chunks > 0) {
        self.extra_fields.add(EXTRA_FIELD_EXECUTION_TIME_ESTIMATES_CHUNK_COUNT_KEY, total_chunks);

        let mut i = 0;
        while (i < total_chunks) {
            let chunk_key = ExecutionTimeObservationChunkKey { chunk_index: i };
            let chunk_data = estimate_chunks[i];
            self.extra_fields.add(chunk_key, chunk_data);
            i = i + 1;
        };
    };
}

/// Return the current validator set
public(package) fun validators(self: &SuiSystemStateInnerV2): &ValidatorSet {
    &self.validators
}

public(package) fun validators_mut(self: &mut SuiSystemStateInnerV2): &mut ValidatorSet {
    &mut self.validators
}

#[test_only]
/// Return the currently active validator by address
public(package) fun active_validator_by_address(
    self: &SuiSystemStateInnerV2,
    validator_address: address,
): &Validator {
    self.validators().get_active_validator_ref(validator_address)
}

#[test_only]
/// Return the currently pending validator by address
public(package) fun pending_validator_by_address(
    self: &SuiSystemStateInnerV2,
    validator_address: address,
): &Validator {
    self.validators().get_pending_validator_ref(validator_address)
}

#[test_only]
/// Return the currently candidate validator by address
public(package) fun candidate_validator_by_address(
    self: &SuiSystemStateInnerV2,
    validator_address: address,
): &Validator {
    self.validators().get_candidate_validator_ref(validator_address)
}

#[test_only]
public(package) fun get_stake_subsidy_distribution_counter(self: &SuiSystemStateInnerV2): u64 {
    self.stake_subsidy.get_distribution_counter()
}

#[test_only]
public(package) fun set_epoch_for_testing(self: &mut SuiSystemStateInnerV2, epoch_num: u64) {
    self.epoch = epoch_num
}

#[test_only]
public(package) fun set_stake_subsidy_distribution_counter(
    self: &mut SuiSystemStateInnerV2,
    counter: u64,
) {
    self.stake_subsidy.set_distribution_counter(counter)
}

#[test_only]
public(package) fun epoch_duration_ms(self: &SuiSystemStateInnerV2): u64 {
    self.parameters.epoch_duration_ms
}

#[test_only]
/// Creates a candidate validator - bypassing the proof of possession check and other
/// metadata validation in the process.
public(package) fun request_add_validator_candidate_for_testing(
    self: &mut SuiSystemStateInnerV2,
    pubkey_bytes: vector<u8>,
    network_pubkey_bytes: vector<u8>,
    worker_pubkey_bytes: vector<u8>,
    proof_of_possession: vector<u8>,
    name: vector<u8>,
    description: vector<u8>,
    image_url: vector<u8>,
    project_url: vector<u8>,
    net_address: vector<u8>,
    p2p_address: vector<u8>,
    primary_address: vector<u8>,
    worker_address: vector<u8>,
    gas_price: u64,
    commission_rate: u64,
    ctx: &mut TxContext,
) {
    let validator = validator::new_for_testing(
        ctx.sender(),
        pubkey_bytes,
        network_pubkey_bytes,
        worker_pubkey_bytes,
        proof_of_possession,
        name,
        description,
        image_url,
        project_url,
        net_address,
        p2p_address,
        primary_address,
        worker_address,
        option::none(),
        gas_price,
        commission_rate,
        false, // not an initial validator active at genesis
        ctx,
    );

    self.validators.request_add_validator_candidate(validator, ctx);
}

macro fun mul_div($a: u64, $b: u64, $c: u64): u64 {
    (($a as u128) * ($b as u128) / ($c as u128)) as u64
}
