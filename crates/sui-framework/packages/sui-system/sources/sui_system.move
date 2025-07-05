// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Sui System State Type Upgrade Guide
/// `SuiSystemState` is a thin wrapper around `SuiSystemStateInner` that provides a versioned interface.
/// The `SuiSystemState` object has a fixed ID 0x5, and the `SuiSystemStateInner` object is stored as a dynamic field.
/// There are a few different ways to upgrade the `SuiSystemStateInner` type:
///
/// The simplest and one that doesn't involve a real upgrade is to just add dynamic fields to the `extra_fields` field
/// of `SuiSystemStateInner` or any of its sub type. This is useful when we are in a rush, or making a small change,
/// or still experimenting a new field.
///
/// To properly upgrade the `SuiSystemStateInner` type, we need to ship a new framework that does the following:
/// 1. Define a new `SuiSystemStateInner`type (e.g. `SuiSystemStateInnerV2`).
/// 2. Define a data migration function that migrates the old `SuiSystemStateInner` to the new one (i.e. SuiSystemStateInnerV2).
/// 3. Replace all uses of `SuiSystemStateInner` with `SuiSystemStateInnerV2` in both sui_system.move and sui_system_state_inner.move,
///    with the exception of the `sui_system_state_inner::create` function, which should always return the genesis type.
/// 4. Inside `load_inner_maybe_upgrade` function, check the current version in the wrapper, and if it's not the latest version,
///   call the data migration function to upgrade the inner object. Make sure to also update the version in the wrapper.
/// A detailed example can be found in sui/tests/framework_upgrades/mock_sui_systems/shallow_upgrade.
/// Along with the Move change, we also need to update the Rust code to support the new type. This includes:
/// 1. Define a new `SuiSystemStateInner` struct type that matches the new Move type, and implement the SuiSystemStateTrait.
/// 2. Update the `SuiSystemState` struct to include the new version as a new enum variant.
/// 3. Update the `get_sui_system_state` function to handle the new version.
/// To test that the upgrade will be successful, we need to modify `sui_system_state_production_upgrade_test` test in
/// protocol_version_tests and trigger a real upgrade using the new framework. We will need to keep this directory as old version,
/// put the new framework in a new directory, and run the test to exercise the upgrade.
///
/// To upgrade Validator type, besides everything above, we also need to:
/// 1. Define a new Validator type (e.g. ValidatorV2).
/// 2. Define a data migration function that migrates the old Validator to the new one (i.e. ValidatorV2).
/// 3. Replace all uses of Validator with ValidatorV2 except the genesis creation function.
/// 4. In validator_wrapper::upgrade_to_latest, check the current version in the wrapper, and if it's not the latest version,
///  call the data migration function to upgrade it.
/// In Rust, we also need to add a new case in `get_validator_from_table`.
/// Note that it is possible to upgrade SuiSystemStateInner without upgrading Validator, but not the other way around.
/// And when we only upgrade SuiSystemStateInner, the version of Validator in the wrapper will not be updated, and hence may become
/// inconsistent with the version of SuiSystemStateInner. This is fine as long as we don't use the Validator version to determine
/// the SuiSystemStateInner version, or vice versa.

module sui_system::sui_system;

use sui::balance::Balance;
use sui::coin::Coin;
use sui::dynamic_field;
use sui::sui::SUI;
use sui::table::Table;
use sui::vec_map::VecMap;
use sui_system::stake_subsidy::StakeSubsidy;
use sui_system::staking_pool::{StakedSui, FungibleStakedSui, PoolTokenExchangeRate};
use sui_system::sui_system_state_inner::{
    Self,
    SystemParameters,
    SuiSystemStateInner,
    SuiSystemStateInnerV2
};
use sui_system::validator::Validator;
use sui_system::validator_cap::UnverifiedValidatorOperationCap;

#[test_only]
use sui::balance;
#[test_only]
use sui_system::validator_set::ValidatorSet;
#[test_only]
use sui::vec_set::VecSet;

public struct SuiSystemState has key {
    id: UID,
    version: u64,
}

const ENotSystemAddress: u64 = 0;
const EWrongInnerVersion: u64 = 1;

// ==== functions that can only be called by genesis ====

/// Create a new SuiSystemState object and make it shared.
/// This function will be called only once in genesis.
public(package) fun create(
    id: UID,
    validators: vector<Validator>,
    storage_fund: Balance<SUI>,
    protocol_version: u64,
    epoch_start_timestamp_ms: u64,
    parameters: SystemParameters,
    stake_subsidy: StakeSubsidy,
    ctx: &mut TxContext,
) {
    let system_state = sui_system_state_inner::create(
        validators,
        storage_fund,
        protocol_version,
        epoch_start_timestamp_ms,
        parameters,
        stake_subsidy,
        ctx,
    );
    let version = sui_system_state_inner::genesis_system_state_version();
    let mut self = SuiSystemState {
        id,
        version,
    };
    dynamic_field::add(&mut self.id, version, system_state);
    transfer::share_object(self);
}

// ==== entry functions ====

/// Can be called by anyone who wishes to become a validator candidate and starts accruing delegated
/// stakes in their staking pool. Once they have at least `MIN_VALIDATOR_JOINING_STAKE` amount of stake they
/// can call `request_add_validator` to officially become an active validator at the next epoch.
/// Aborts if the caller is already a pending or active validator, or a validator candidate.
/// Note: `proof_of_possession` MUST be a valid signature using sui_address and protocol_pubkey_bytes.
/// To produce a valid PoP, run [fn test_proof_of_possession].
public entry fun request_add_validator_candidate(
    wrapper: &mut SuiSystemState,
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
    wrapper
        .load_system_state_mut()
        .request_add_validator_candidate(
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
        )
}

/// Called by a validator candidate to remove themselves from the candidacy. After this call
/// their staking pool becomes deactivate.
public entry fun request_remove_validator_candidate(
    wrapper: &mut SuiSystemState,
    ctx: &mut TxContext,
) {
    wrapper.load_system_state_mut().request_remove_validator_candidate(ctx)
}

/// Called by a validator candidate to add themselves to the active validator set beginning next epoch.
/// Aborts if the validator is a duplicate with one of the pending or active validators, or if the amount of
/// stake the validator has doesn't meet the min threshold, or if the number of new validators for the next
/// epoch has already reached the maximum.
public entry fun request_add_validator(wrapper: &mut SuiSystemState, ctx: &mut TxContext) {
    wrapper.load_system_state_mut().request_add_validator(ctx)
}

/// A validator can call this function to request a removal in the next epoch.
/// We use the sender of `ctx` to look up the validator
/// (i.e. sender must match the sui_address in the validator).
/// At the end of the epoch, the `validator` object will be returned to the sui_address
/// of the validator.
public entry fun request_remove_validator(wrapper: &mut SuiSystemState, ctx: &mut TxContext) {
    wrapper.load_system_state_mut().request_remove_validator(ctx)
}

/// A validator can call this entry function to submit a new gas price quote, to be
/// used for the reference gas price calculation at the end of the epoch.
public entry fun request_set_gas_price(
    wrapper: &mut SuiSystemState,
    cap: &UnverifiedValidatorOperationCap,
    new_gas_price: u64,
) {
    wrapper.load_system_state_mut().request_set_gas_price(cap, new_gas_price)
}

/// This entry function is used to set new gas price for candidate validators
public entry fun set_candidate_validator_gas_price(
    wrapper: &mut SuiSystemState,
    cap: &UnverifiedValidatorOperationCap,
    new_gas_price: u64,
) {
    wrapper.load_system_state_mut().set_candidate_validator_gas_price(cap, new_gas_price)
}

/// A validator can call this entry function to set a new commission rate, updated at the end of
/// the epoch.
public entry fun request_set_commission_rate(
    wrapper: &mut SuiSystemState,
    new_commission_rate: u64,
    ctx: &mut TxContext,
) {
    wrapper.load_system_state_mut().request_set_commission_rate(new_commission_rate, ctx)
}

/// This entry function is used to set new commission rate for candidate validators
public entry fun set_candidate_validator_commission_rate(
    wrapper: &mut SuiSystemState,
    new_commission_rate: u64,
    ctx: &mut TxContext,
) {
    wrapper
        .load_system_state_mut()
        .set_candidate_validator_commission_rate(new_commission_rate, ctx)
}

/// Add stake to a validator's staking pool.
public entry fun request_add_stake(
    wrapper: &mut SuiSystemState,
    stake: Coin<SUI>,
    validator_address: address,
    ctx: &mut TxContext,
) {
    let staked_sui = request_add_stake_non_entry(wrapper, stake, validator_address, ctx);
    transfer::public_transfer(staked_sui, ctx.sender());
}

/// The non-entry version of `request_add_stake`, which returns the staked SUI instead of transferring it to the sender.
public fun request_add_stake_non_entry(
    wrapper: &mut SuiSystemState,
    stake: Coin<SUI>,
    validator_address: address,
    ctx: &mut TxContext,
): StakedSui {
    wrapper.load_system_state_mut().request_add_stake(stake, validator_address, ctx)
}

/// Add stake to a validator's staking pool using multiple coins.
public entry fun request_add_stake_mul_coin(
    wrapper: &mut SuiSystemState,
    stakes: vector<Coin<SUI>>,
    stake_amount: option::Option<u64>,
    validator_address: address,
    ctx: &mut TxContext,
) {
    let staked_sui = wrapper
        .load_system_state_mut()
        .request_add_stake_mul_coin(stakes, stake_amount, validator_address, ctx);

    transfer::public_transfer(staked_sui, ctx.sender());
}

/// Withdraw stake from a validator's staking pool.
public entry fun request_withdraw_stake(
    wrapper: &mut SuiSystemState,
    staked_sui: StakedSui,
    ctx: &mut TxContext,
) {
    let withdrawn_stake = wrapper.request_withdraw_stake_non_entry(staked_sui, ctx);
    transfer::public_transfer(withdrawn_stake.into_coin(ctx), ctx.sender());
}

/// Convert StakedSui into a FungibleStakedSui object.
public fun convert_to_fungible_staked_sui(
    wrapper: &mut SuiSystemState,
    staked_sui: StakedSui,
    ctx: &mut TxContext,
): FungibleStakedSui {
    wrapper.load_system_state_mut().convert_to_fungible_staked_sui(staked_sui, ctx)
}

/// Convert FungibleStakedSui into a StakedSui object.
public fun redeem_fungible_staked_sui(
    wrapper: &mut SuiSystemState,
    fungible_staked_sui: FungibleStakedSui,
    ctx: &TxContext,
): Balance<SUI> {
    wrapper.load_system_state_mut().redeem_fungible_staked_sui(fungible_staked_sui, ctx)
}

/// Non-entry version of `request_withdraw_stake` that returns the withdrawn SUI instead of transferring it to the sender.
public fun request_withdraw_stake_non_entry(
    wrapper: &mut SuiSystemState,
    staked_sui: StakedSui,
    ctx: &mut TxContext,
): Balance<SUI> {
    wrapper.load_system_state_mut().request_withdraw_stake(staked_sui, ctx)
}

/// Report a validator as a bad or non-performant actor in the system.
/// Succeeds if all the following are satisfied:
/// 1. both the reporter in `cap` and the input `reportee_addr` are active validators.
/// 2. reporter and reportee not the same address.
/// 3. the cap object is still valid.
/// This function is idempotent.
public entry fun report_validator(
    wrapper: &mut SuiSystemState,
    cap: &UnverifiedValidatorOperationCap,
    reportee_addr: address,
) {
    wrapper.load_system_state_mut().report_validator(cap, reportee_addr)
}

/// Undo a `report_validator` action. Aborts if
/// 1. the reportee is not a currently active validator or
/// 2. the sender has not previously reported the `reportee_addr`, or
/// 3. the cap is not valid
public entry fun undo_report_validator(
    wrapper: &mut SuiSystemState,
    cap: &UnverifiedValidatorOperationCap,
    reportee_addr: address,
) {
    wrapper.load_system_state_mut().undo_report_validator(cap, reportee_addr)
}

// ==== validator metadata management functions ====

/// Create a new `UnverifiedValidatorOperationCap`, transfer it to the
/// validator and registers it. The original object is thus revoked.
public entry fun rotate_operation_cap(self: &mut SuiSystemState, ctx: &mut TxContext) {
    self.load_system_state_mut().rotate_operation_cap(ctx)
}

/// Update a validator's name.
public entry fun update_validator_name(
    self: &mut SuiSystemState,
    name: vector<u8>,
    ctx: &TxContext,
) {
    self.load_system_state_mut().update_validator_name(name, ctx)
}

/// Update a validator's description
public entry fun update_validator_description(
    self: &mut SuiSystemState,
    description: vector<u8>,
    ctx: &TxContext,
) {
    self.load_system_state_mut().update_validator_description(description, ctx)
}

/// Update a validator's image url
public entry fun update_validator_image_url(
    self: &mut SuiSystemState,
    image_url: vector<u8>,
    ctx: &TxContext,
) {
    self.load_system_state_mut().update_validator_image_url(image_url, ctx)
}

/// Update a validator's project url
public entry fun update_validator_project_url(
    self: &mut SuiSystemState,
    project_url: vector<u8>,
    ctx: &TxContext,
) {
    self.load_system_state_mut().update_validator_project_url(project_url, ctx)
}

/// Update a validator's network address.
/// The change will only take effects starting from the next epoch.
public entry fun update_validator_next_epoch_network_address(
    self: &mut SuiSystemState,
    network_address: vector<u8>,
    ctx: &TxContext,
) {
    self.load_system_state_mut().update_validator_next_epoch_network_address(network_address, ctx)
}

/// Update candidate validator's network address.
public entry fun update_candidate_validator_network_address(
    self: &mut SuiSystemState,
    network_address: vector<u8>,
    ctx: &TxContext,
) {
    self.load_system_state_mut().update_candidate_validator_network_address(network_address, ctx)
}

/// Update a validator's p2p address.
/// The change will only take effects starting from the next epoch.
public entry fun update_validator_next_epoch_p2p_address(
    self: &mut SuiSystemState,
    p2p_address: vector<u8>,
    ctx: &TxContext,
) {
    self.load_system_state_mut().update_validator_next_epoch_p2p_address(p2p_address, ctx)
}

/// Update candidate validator's p2p address.
public entry fun update_candidate_validator_p2p_address(
    self: &mut SuiSystemState,
    p2p_address: vector<u8>,
    ctx: &TxContext,
) {
    self.load_system_state_mut().update_candidate_validator_p2p_address(p2p_address, ctx)
}

/// Update a validator's narwhal primary address.
/// The change will only take effects starting from the next epoch.
public entry fun update_validator_next_epoch_primary_address(
    self: &mut SuiSystemState,
    primary_address: vector<u8>,
    ctx: &TxContext,
) {
    self.load_system_state_mut().update_validator_next_epoch_primary_address(primary_address, ctx)
}

/// Update candidate validator's narwhal primary address.
public entry fun update_candidate_validator_primary_address(
    self: &mut SuiSystemState,
    primary_address: vector<u8>,
    ctx: &TxContext,
) {
    self.load_system_state_mut().update_candidate_validator_primary_address(primary_address, ctx)
}

/// Update a validator's narwhal worker address.
/// The change will only take effects starting from the next epoch.
public entry fun update_validator_next_epoch_worker_address(
    self: &mut SuiSystemState,
    worker_address: vector<u8>,
    ctx: &TxContext,
) {
    self.load_system_state_mut().update_validator_next_epoch_worker_address(worker_address, ctx)
}

/// Update candidate validator's narwhal worker address.
public entry fun update_candidate_validator_worker_address(
    self: &mut SuiSystemState,
    worker_address: vector<u8>,
    ctx: &TxContext,
) {
    self.load_system_state_mut().update_candidate_validator_worker_address(worker_address, ctx)
}

/// Update a validator's public key of protocol key and proof of possession.
/// The change will only take effects starting from the next epoch.
public entry fun update_validator_next_epoch_protocol_pubkey(
    self: &mut SuiSystemState,
    protocol_pubkey: vector<u8>,
    proof_of_possession: vector<u8>,
    ctx: &TxContext,
) {
    self
        .load_system_state_mut()
        .update_validator_next_epoch_protocol_pubkey(protocol_pubkey, proof_of_possession, ctx)
}

/// Update candidate validator's public key of protocol key and proof of possession.
public entry fun update_candidate_validator_protocol_pubkey(
    self: &mut SuiSystemState,
    protocol_pubkey: vector<u8>,
    proof_of_possession: vector<u8>,
    ctx: &TxContext,
) {
    self
        .load_system_state_mut()
        .update_candidate_validator_protocol_pubkey(protocol_pubkey, proof_of_possession, ctx)
}

/// Update a validator's public key of worker key.
/// The change will only take effects starting from the next epoch.
public entry fun update_validator_next_epoch_worker_pubkey(
    self: &mut SuiSystemState,
    worker_pubkey: vector<u8>,
    ctx: &TxContext,
) {
    self.load_system_state_mut().update_validator_next_epoch_worker_pubkey(worker_pubkey, ctx)
}

/// Update candidate validator's public key of worker key.
public entry fun update_candidate_validator_worker_pubkey(
    self: &mut SuiSystemState,
    worker_pubkey: vector<u8>,
    ctx: &TxContext,
) {
    self.load_system_state_mut().update_candidate_validator_worker_pubkey(worker_pubkey, ctx)
}

/// Update a validator's public key of network key.
/// The change will only take effects starting from the next epoch.
public entry fun update_validator_next_epoch_network_pubkey(
    self: &mut SuiSystemState,
    network_pubkey: vector<u8>,
    ctx: &TxContext,
) {
    self.load_system_state_mut().update_validator_next_epoch_network_pubkey(network_pubkey, ctx)
}

/// Update candidate validator's public key of network key.
public entry fun update_candidate_validator_network_pubkey(
    self: &mut SuiSystemState,
    network_pubkey: vector<u8>,
    ctx: &TxContext,
) {
    self.load_system_state_mut().update_candidate_validator_network_pubkey(network_pubkey, ctx)
}

public fun validator_address_by_pool_id(wrapper: &mut SuiSystemState, pool_id: &ID): address {
    wrapper.load_system_state_mut().validator_address_by_pool_id(pool_id)
}

/// Getter of the pool token exchange rate of a staking pool. Works for both active and inactive pools.
public fun pool_exchange_rates(
    wrapper: &mut SuiSystemState,
    pool_id: &ID,
): &Table<u64, PoolTokenExchangeRate> {
    wrapper.load_system_state_mut().pool_exchange_rates(pool_id)
}

/// Getter returning addresses of the currently active validators.
public fun active_validator_addresses(wrapper: &mut SuiSystemState): vector<address> {
    wrapper.load_system_state_mut().active_validator_addresses()
}

/// Calculate the rewards for a given staked SUI object.
/// Used in the package, and can be dev-inspected.
public(package) fun calculate_rewards(
    self: &mut SuiSystemState,
    staked_sui: &StakedSui,
    ctx: &TxContext,
): u64 {
    let system_state = self.load_system_state_mut();

    system_state
        .validators_mut()
        .validator_by_pool_id(&staked_sui.pool_id())
        .get_staking_pool_ref()
        .calculate_rewards(staked_sui, ctx.epoch())
}

#[allow(unused_function)]
/// This function should be called at the end of an epoch, and advances the system to the next epoch.
/// It does the following things:
/// 1. Add storage charge to the storage fund.
/// 2. Burn the storage rebates from the storage fund. These are already refunded to transaction sender's
///    gas coins.
/// 3. Distribute computation charge to validator stake.
/// 4. Update all validators.
fun advance_epoch(
    storage_reward: Balance<SUI>,
    computation_reward: Balance<SUI>,
    wrapper: &mut SuiSystemState,
    new_epoch: u64,
    next_protocol_version: u64,
    storage_rebate: u64,
    non_refundable_storage_fee: u64,
    storage_fund_reinvest_rate: u64, // share of storage fund's rewards that's reinvested
    // into storage fund, in basis point.
    reward_slashing_rate: u64, // how much rewards are slashed to punish a validator, in bps.
    epoch_start_timestamp_ms: u64, // Timestamp of the epoch start
    ctx: &mut TxContext,
): Balance<SUI> {
    // Validator will make a special system call with sender set as 0x0.
    assert!(ctx.sender() == @0x0, ENotSystemAddress);
    let storage_rebate = wrapper
        .load_system_state_mut()
        .advance_epoch(
            new_epoch,
            next_protocol_version,
            storage_reward,
            computation_reward,
            storage_rebate,
            non_refundable_storage_fee,
            storage_fund_reinvest_rate,
            reward_slashing_rate,
            epoch_start_timestamp_ms,
            ctx,
        );

    storage_rebate
}

fun load_system_state(self: &mut SuiSystemState): &SuiSystemStateInnerV2 {
    load_inner_maybe_upgrade(self)
}

fun load_system_state_mut(self: &mut SuiSystemState): &mut SuiSystemStateInnerV2 {
    load_inner_maybe_upgrade(self)
}

fun load_inner_maybe_upgrade(self: &mut SuiSystemState): &mut SuiSystemStateInnerV2 {
    if (self.version == 1) {
        let v1: SuiSystemStateInner = dynamic_field::remove(&mut self.id, self.version);
        let v2 = v1.v1_to_v2();
        self.version = 2;
        dynamic_field::add(&mut self.id, self.version, v2);
    };

    let inner: &mut SuiSystemStateInnerV2 = dynamic_field::borrow_mut(
        &mut self.id,
        self.version,
    );
    assert!(inner.system_state_version() == self.version, EWrongInnerVersion);
    inner
}

#[allow(unused_function)]
/// Returns the voting power of the active validators, values are voting power in the scale of 10000.
fun validator_voting_powers(wrapper: &mut SuiSystemState): VecMap<address, u64> {
    wrapper.load_system_state().active_validator_voting_powers()
}

#[allow(unused_function)]
/// Saves the given execution time estimate blob to the SuiSystemState object, for system use
/// at the start of the next epoch.
fun store_execution_time_estimates(wrapper: &mut SuiSystemState, estimates_bytes: vector<u8>) {
    wrapper.load_system_state_mut().store_execution_time_estimates(estimates_bytes)
}

#[test_only]
public fun validator_voting_powers_for_testing(wrapper: &mut SuiSystemState): VecMap<address, u64> {
    wrapper.validator_voting_powers()
}

#[test_only]
/// Return the current epoch number. Useful for applications that need a coarse-grained concept of time,
/// since epochs are ever-increasing and epoch changes are intended to happen every 24 hours.
public fun epoch(wrapper: &mut SuiSystemState): u64 {
    wrapper.load_system_state_mut().epoch()
}

#[test_only]
/// Returns unix timestamp of the start of current epoch
public fun epoch_start_timestamp_ms(wrapper: &mut SuiSystemState): u64 {
    wrapper.load_system_state_mut().epoch_start_timestamp_ms()
}

#[test_only]
/// Returns the total amount staked with `validator_addr`.
/// Aborts if `validator_addr` is not an active validator.
public fun validator_stake_amount(wrapper: &mut SuiSystemState, validator_addr: address): u64 {
    wrapper.load_system_state_mut().validator_stake_amount(validator_addr)
}

#[test_only]
/// Returns the staking pool id of a given validator.
/// Aborts if `validator_addr` is not an active validator.
public fun validator_staking_pool_id(wrapper: &mut SuiSystemState, validator_addr: address): ID {
    wrapper.load_system_state_mut().validator_staking_pool_id(validator_addr)
}

#[test_only]
/// Returns reference to the staking pool mappings that map pool ids to active validator addresses
public fun validator_staking_pool_mappings(wrapper: &mut SuiSystemState): &Table<ID, address> {
    wrapper.load_system_state_mut().validator_staking_pool_mappings()
}

#[test_only]
/// Returns all the validators who are currently reporting `addr`
public fun get_reporters_of(wrapper: &mut SuiSystemState, addr: address): VecSet<address> {
    wrapper.load_system_state_mut().get_reporters_of(addr)
}

#[test_only]
/// Return the current validator set
public fun validators(wrapper: &mut SuiSystemState): &ValidatorSet {
    wrapper.load_system_state_mut().validators()
}

#[test_only]
/// Return a mutable reference to the validator set
public fun validators_mut(wrapper: &mut SuiSystemState): &mut ValidatorSet {
    wrapper.load_system_state_mut().validators_mut()
}

#[test_only]
/// Return the currently active validator by address
public fun active_validator_by_address(
    self: &mut SuiSystemState,
    validator_address: address,
): &Validator {
    self.validators().get_active_validator_ref(validator_address)
}

#[test_only]
/// Return the currently pending validator by address
public fun pending_validator_by_address(
    self: &mut SuiSystemState,
    validator_address: address,
): &Validator {
    self.validators().get_pending_validator_ref(validator_address)
}

#[test_only]
/// Return the currently candidate validator by address
public fun candidate_validator_by_address(
    self: &mut SuiSystemState,
    validator_address: address,
): &Validator {
    self.validators().get_candidate_validator_ref(validator_address)
}

#[test_only]
public fun set_epoch_for_testing(wrapper: &mut SuiSystemState, epoch_num: u64) {
    wrapper.load_system_state_mut().set_epoch_for_testing(epoch_num)
}

#[test_only]
public fun request_add_validator_for_testing(wrapper: &mut SuiSystemState, ctx: &TxContext) {
    wrapper.load_system_state_mut().request_add_validator(ctx)
}

#[test_only]
public fun get_storage_fund_total_balance(wrapper: &mut SuiSystemState): u64 {
    wrapper.load_system_state_mut().get_storage_fund_total_balance()
}

#[test_only]
public fun get_storage_fund_object_rebates(wrapper: &mut SuiSystemState): u64 {
    wrapper.load_system_state_mut().get_storage_fund_object_rebates()
}

#[test_only]
public fun get_stake_subsidy_distribution_counter(wrapper: &mut SuiSystemState): u64 {
    wrapper.load_system_state_mut().get_stake_subsidy_distribution_counter()
}

#[test_only]
public fun set_stake_subsidy_distribution_counter(wrapper: &mut SuiSystemState, counter: u64) {
    wrapper.load_system_state_mut().set_stake_subsidy_distribution_counter(counter)
}

#[test_only]
public fun inner_mut_for_testing(wrapper: &mut SuiSystemState): &mut SuiSystemStateInnerV2 {
    wrapper.load_system_state_mut()
}

// CAUTION: THIS CODE IS ONLY FOR TESTING AND THIS MACRO MUST NEVER EVER BE REMOVED.  Creates a
// candidate validator - bypassing the proof of possession check and other metadata validation
// in the process.
#[test_only]
public entry fun request_add_validator_candidate_for_testing(
    wrapper: &mut SuiSystemState,
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
    wrapper
        .load_system_state_mut()
        .request_add_validator_candidate_for_testing(
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
        )
}

// CAUTION: THIS CODE IS ONLY FOR TESTING AND THIS MACRO MUST NEVER EVER BE REMOVED.
#[test_only]
public(package) fun advance_epoch_for_testing(
    wrapper: &mut SuiSystemState,
    new_epoch: u64,
    next_protocol_version: u64,
    storage_charge: u64,
    computation_charge: u64,
    storage_rebate: u64,
    non_refundable_storage_fee: u64,
    storage_fund_reinvest_rate: u64,
    reward_slashing_rate: u64,
    epoch_start_timestamp_ms: u64,
    ctx: &mut TxContext,
): Balance<SUI> {
    let storage_reward = balance::create_for_testing(storage_charge);
    let computation_reward = balance::create_for_testing(computation_charge);
    let storage_rebate = advance_epoch(
        storage_reward,
        computation_reward,
        wrapper,
        new_epoch,
        next_protocol_version,
        storage_rebate,
        non_refundable_storage_fee,
        storage_fund_reinvest_rate,
        reward_slashing_rate,
        epoch_start_timestamp_ms,
        ctx,
    );
    storage_rebate
}
