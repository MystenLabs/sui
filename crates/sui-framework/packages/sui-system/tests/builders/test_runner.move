// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// Test Runner is a context-specific wrapper around the `Scenario` struct, which
/// provides a set of convenience methods for testing the Sui System.
module sui_system::test_runner;

use sui::balance::{Self, Balance};
use sui::coin::{Self, Coin};
use sui::sui::SUI;
use sui::test_scenario::{Self, Scenario};
use sui_system::stake_subsidy;
use sui_system::staking_pool::StakedSui;
use sui_system::sui_system::{Self, SuiSystemState};
use sui_system::sui_system_state_inner;
use sui_system::validator::Validator;
use sui_system::validator_builder::{Self, ValidatorBuilder};

const MIST_PER_SUI: u64 = 1_000_000_000;

// === Test Runner Builder ===

public struct TestRunnerBuilder {
    /// Mutually exclusive with `validators_number`
    validators: Option<vector<ValidatorBuilder>>,
    sui_supply_amount: Option<u64>,
    storage_fund_amount: Option<u64>,
    /// Mutually exclusive with `validators`
    validators_count: Option<u64>,
    validators_initial_stake: Option<u64>,
}

public fun new(): TestRunnerBuilder {
    TestRunnerBuilder {
        validators: option::none(),
        validators_count: option::none(),
        sui_supply_amount: option::none(),
        storage_fund_amount: option::none(),
        validators_initial_stake: option::none(),
    }
}

public fun build(builder: TestRunnerBuilder): TestRunner {
    let mut scenario = test_scenario::begin(@0);

    let TestRunnerBuilder {
        validators,
        sui_supply_amount,
        storage_fund_amount,
        validators_count,
        validators_initial_stake,
    } = builder;

    let validators = validators.destroy_or!({
        vector::tabulate!(validators_count.destroy_or!(4), |_| {
            validator_builder::new().initial_stake(validators_initial_stake.destroy_or!(1_000_000))
        })
    });

    let system_parameters = sui_system_state_inner::create_system_parameters(
        42, // epoch_duration_ms, doesn't matter what number we put here
        0, // stake_subsidy_start_epoch
        150, // max_validator_count
        1, // min_validator_joining_stake
        1, // validator_low_stake_threshold
        0, // validator_very_low_stake_threshold
        7, // validator_low_stake_grace_period
        scenario.ctx(),
    );

    let stake_subsidy = stake_subsidy::create(
        balance::create_for_testing<SUI>(sui_supply_amount.destroy_or!(1000) * MIST_PER_SUI), // sui_supply
        0, // stake subsidy initial distribution amount
        10, // stake_subsidy_period_length
        0, // stake_subsidy_decrease_rate
        scenario.ctx(),
    );

    sui_system::create(
        object::new(scenario.ctx()), // it doesn't matter what ID sui system state has in tests
        validators.map!(|v| v.is_active_at_genesis(true).build(scenario.ctx())),
        balance::create_for_testing<SUI>(storage_fund_amount.destroy_or!(0) * MIST_PER_SUI), // storage_fund
        1, // protocol version
        0, // chain_start_timestamp_ms
        system_parameters,
        stake_subsidy,
        scenario.ctx(),
    );

    TestRunner {
        scenario,
        sender: @0,
    }
}

public fun validators(
    mut builder: TestRunnerBuilder,
    validators: vector<ValidatorBuilder>,
): TestRunnerBuilder {
    builder.validators.fill(validators);
    builder
}

public fun validators_count(
    mut builder: TestRunnerBuilder,
    validators_count: u64,
): TestRunnerBuilder {
    builder.validators_count = option::some(validators_count);
    builder
}

public fun sui_supply_amount(
    mut builder: TestRunnerBuilder,
    sui_supply_amount: u64,
): TestRunnerBuilder {
    builder.sui_supply_amount = option::some(sui_supply_amount);
    builder
}

public fun storage_fund_amount(
    mut builder: TestRunnerBuilder,
    storage_fund_amount: u64,
): TestRunnerBuilder {
    builder.storage_fund_amount = option::some(storage_fund_amount);
    builder
}

public fun validators_initial_stake(
    mut builder: TestRunnerBuilder,
    validators_initial_stake: u64,
): TestRunnerBuilder {
    builder.validators_initial_stake = option::some(validators_initial_stake);
    builder
}

// === Advance Epoch Options ===

public struct AdvanceEpochOptions has drop {
    protocol_version: Option<u64>,
    storage_charge: Option<u64>,
    computation_charge: Option<u64>,
    storage_rebate: Option<u64>,
    non_refundable_storage_fee: Option<u64>,
    computation_rebate: Option<u64>,
    reward_slashing_rate: Option<u64>,
    epoch_start_time: Option<u64>,
}

public fun advance_epoch_opts(_: &TestRunner): AdvanceEpochOptions {
    AdvanceEpochOptions {
        protocol_version: option::none(),
        storage_charge: option::none(),
        computation_charge: option::none(),
        storage_rebate: option::none(),
        non_refundable_storage_fee: option::none(),
        computation_rebate: option::none(),
        reward_slashing_rate: option::none(),
        epoch_start_time: option::none(),
    }
}

public fun protocol_version(
    mut opts: AdvanceEpochOptions,
    protocol_version: u64,
): AdvanceEpochOptions {
    opts.protocol_version = option::some(protocol_version);
    opts
}

public fun storage_charge(mut opts: AdvanceEpochOptions, storage_charge: u64): AdvanceEpochOptions {
    opts.storage_charge = option::some(storage_charge);
    opts
}

public fun computation_charge(
    mut opts: AdvanceEpochOptions,
    computation_charge: u64,
): AdvanceEpochOptions {
    opts.computation_charge = option::some(computation_charge);
    opts
}

public fun storage_rebate(mut opts: AdvanceEpochOptions, storage_rebate: u64): AdvanceEpochOptions {
    opts.storage_rebate = option::some(storage_rebate);
    opts
}

public fun non_refundable_storage_fee(
    mut opts: AdvanceEpochOptions,
    non_refundable_storage_fee: u64,
): AdvanceEpochOptions {
    opts.non_refundable_storage_fee = option::some(non_refundable_storage_fee);
    opts
}

public fun computation_rebate(
    mut opts: AdvanceEpochOptions,
    computation_rebate: u64,
): AdvanceEpochOptions {
    opts.computation_rebate = option::some(computation_rebate);
    opts
}

public fun reward_slashing_rate(
    mut opts: AdvanceEpochOptions,
    reward_slashing_rate: u64,
): AdvanceEpochOptions {
    opts.reward_slashing_rate = option::some(reward_slashing_rate);
    opts
}

public fun epoch_start_time(
    mut opts: AdvanceEpochOptions,
    epoch_start_time: u64,
): AdvanceEpochOptions {
    opts.epoch_start_time = option::some(epoch_start_time);
    opts
}

// === Test Runner ===

/// Runner for tests, provides methods to access objects and methods of the system.
public struct TestRunner {
    scenario: Scenario,
    sender: address,
}

/// Set the sender of the next transaction.
public fun set_sender(runner: &mut TestRunner, sender: address): &mut TestRunner {
    runner.scenario.next_tx(sender);
    runner.sender = sender;
    runner
}

/// Get the current transaction context.
public fun ctx(runner: &mut TestRunner): &mut TxContext { runner.scenario.ctx() }

public fun scenario_mut(runner: &mut TestRunner): &mut Scenario { &mut runner.scenario }

public fun keep<T: key + store>(runner: &TestRunner, object: T) {
    transfer::public_transfer(object, runner.sender);
}

public fun sender(runner: &mut TestRunner): address { runner.sender }

public fun mint(amount: u64): Balance<SUI> {
    balance::create_for_testing(amount * MIST_PER_SUI)
}

public fun destroy<T>(v: T) {
    sui::test_utils::destroy(v);
}

/// Finish the test runner.
public fun finish(runner: TestRunner) {
    let TestRunner { scenario, .. } = runner;
    scenario.end();
}

// === Macros ===

public macro fun scenario_fn($runner: &mut TestRunner, $f: |&mut Scenario|) {
    let sender = sender($runner);
    let scenario = scenario_mut($runner);
    scenario.next_tx(sender);
    $f(scenario);
}

public macro fun owned_tx<$Object>($runner: &mut TestRunner, $f: |$Object|) {
    let sender = sender($runner);
    let scenario = scenario_mut($runner);
    scenario.next_tx(sender);
    $f(scenario.take_from_sender<$Object>());
}

/// Run a transaction on the system state
public macro fun system_tx($runner: &mut TestRunner, $f: |&mut SuiSystemState, &mut TxContext|) {
    let sender = sender($runner);
    let scenario = scenario_mut($runner);
    scenario.next_tx(sender);
    let mut system_state = scenario.take_shared<SuiSystemState>();
    $f(&mut system_state, scenario.ctx());
    test_scenario::return_shared(system_state);
}

/// Advance the epoch of the system state. Takes an optional `AdvanceEpochOptions` struct
/// to configure the epoch advance. Returns the storage rebate balance.
///
/// ```rust
/// // default, no rewards
/// runner.advance_epoch(option::none());
///
/// // custom options
/// let opts = runner.advance_epoch_opts()
///     .storage_charge(100)
///     .computation_charge(200)
///     .storage_rebate(10)
///     .reward_slashing_rate(10);
///
/// runner.advance_epoch(option::some(opts));
/// ```
public fun advance_epoch(
    runner: &mut TestRunner,
    options: Option<AdvanceEpochOptions>,
): Balance<SUI> {
    let sender = runner.sender;
    runner.set_sender(@0);
    let storage_rebate_balance;
    let options = options.destroy_or!(runner.advance_epoch_opts());

    let AdvanceEpochOptions {
        protocol_version,
        storage_charge,
        computation_charge,
        storage_rebate,
        non_refundable_storage_fee,
        computation_rebate,
        reward_slashing_rate,
        epoch_start_time,
    } = options;

    runner.system_tx!(|system, ctx| {
        let new_epoch = ctx.epoch() + 1;
        storage_rebate_balance =
            system.advance_epoch_for_testing(
                new_epoch,
                protocol_version.destroy_or!(1),
                storage_charge.destroy_or!(0) * MIST_PER_SUI,
                computation_charge.destroy_or!(0) * MIST_PER_SUI,
                storage_rebate.destroy_or!(0),
                non_refundable_storage_fee.destroy_or!(0),
                computation_rebate.destroy_or!(0),
                reward_slashing_rate.destroy_or!(0),
                epoch_start_time.destroy_or!(0),
                ctx,
            );
    });

    runner.scenario.next_epoch(@0);
    runner.set_sender(sender);
    storage_rebate_balance
}

/// Call the `request_add_stake` function on the system state.
public fun stake_with(runner: &mut TestRunner, validator: address, amount: u64) {
    let TestRunner { scenario, sender } = runner;
    scenario.next_tx(*sender);
    runner.system_tx!(|system, ctx| {
        system.request_add_stake(
            coin::mint_for_testing(amount * MIST_PER_SUI, ctx),
            validator,
            ctx,
        );
    });
}

/// Call the `request_withdraw_stake` function on the system state.
public fun unstake(runner: &mut TestRunner, staked_sui_idx: u64) {
    let sender = runner.sender;
    runner.set_sender(sender);
    let stake_sui_ids = runner.scenario.ids_for_sender<StakedSui>();
    let staked_sui = runner.scenario.take_from_sender_by_id(stake_sui_ids[staked_sui_idx]);
    runner.system_tx!(|system, ctx| {
        system.request_withdraw_stake(staked_sui, ctx);
    });
}

// === Validator Management ===

/// Add a validator candidate to the system state.
public fun add_validator_candidate(runner: &mut TestRunner, validator: Validator) {
    let sender = runner.sender;
    runner.scenario.next_tx(validator.sui_address());
    runner.system_tx!(|system, ctx| {
        system.validators_mut().request_add_validator_candidate(validator, ctx);
    });
    runner.scenario.next_tx(sender);
}

/// Remove a validator candidate from the system state.
public fun remove_validator_candidate(runner: &mut TestRunner) {
    runner.system_tx!(|system, ctx| {
        system.validators_mut().request_remove_validator_candidate(ctx);
    });
}

/// Requests the addition of a validator to the active validator set beginning next epoch.
/// The sender of the transaction must match the validator's address.
public fun add_validator(runner: &mut TestRunner) {
    runner.system_tx!(|system, ctx| {
        system.validators_mut().request_add_validator(ctx);
    });
}

/// Remove a validator from the system state.
public fun remove_validator(runner: &mut TestRunner) {
    runner.system_tx!(|system, ctx| {
        system.validators_mut().request_remove_validator(ctx);
    });
}

/// Get the sum of the balances of all the SUI coins in the sender's account.
public fun sui_balance(runner: &mut TestRunner): u64 {
    let sender = runner.sender;
    let scenario = runner.scenario_mut();
    scenario.next_tx(sender);
    scenario.ids_for_sender<Coin<SUI>>().fold!(0, |mut sum, coin_id| {
        let coin = scenario.take_from_sender_by_id<Coin<SUI>>(coin_id);
        sum = sum + coin.value();
        scenario.return_to_sender(coin);
        sum
    })
}

#[test]
fun test_runner_builder() {
    let mut runner = Self::new().validators_count(4).validators_initial_stake(1_000_000).build();
    let validator_addr;

    runner.system_tx!(|system, _ctx| {
        assert!(system.validators().active_validators().length() == 4);
        validator_addr = system.validators().active_validator_addresses()[0];
    });

    runner.set_sender(@1);
    runner.stake_with(validator_addr, 1_000_000);
    runner.advance_epoch(option::none()).destroy_for_testing();
    runner.unstake(0);
    runner.finish();
}
