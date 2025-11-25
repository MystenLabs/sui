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
    protocol_version: Option<u64>,
    stake_distribution_counter: Option<u64>,
    start_epoch: Option<u64>,
    epoch_duration: Option<u64>,
    low_stake_grace_period: Option<u64>,
}

public fun new(): TestRunnerBuilder {
    TestRunnerBuilder {
        validators: option::none(),
        validators_count: option::none(),
        sui_supply_amount: option::none(),
        storage_fund_amount: option::none(),
        validators_initial_stake: option::none(),
        protocol_version: option::none(),
        stake_distribution_counter: option::none(),
        epoch_duration: option::none(),
        start_epoch: option::none(),
        low_stake_grace_period: option::none(),
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
        protocol_version,
        stake_distribution_counter,
        epoch_duration,
        start_epoch,
        low_stake_grace_period,
    } = builder;

    let validators = validators.destroy_or!({
        vector::tabulate!(validators_count.destroy_or!(4), |_| {
            validator_builder::new().initial_stake(validators_initial_stake.destroy_or!(100))
        })
    });

    // create system parameters
    // TODO: make this configurable
    let system_parameters = sui_system_state_inner::create_system_parameters(
        epoch_duration.destroy_or!(42), // epoch_duration_ms, doesn't matter what number we put here
        0, // stake_subsidy_start_epoch
        150, // max_validator_count
        1, // DEPRECATED: min_validator_joining_stake
        1, // DEPRECATED: validator_low_stake_threshold
        0, // DEPRECATED: validator_very_low_stake_threshold
        low_stake_grace_period.destroy_or!(7), // validator_low_stake_grace_period
        scenario.ctx(),
    );

    // create stake subsidy
    let stake_subsidy = stake_subsidy::create(
        balance::create_for_testing<SUI>(sui_supply_amount.destroy_or!(1000) * MIST_PER_SUI), // sui_supply
        0, // stake subsidy initial distribution amount
        10, // stake_subsidy_period_length
        0, // stake_subsidy_decrease_rate
        scenario.ctx(),
    );

    let validators = validators.map!(
        |v| v
            .is_active_at_genesis(true)
            .try_initial_stake(validators_initial_stake.destroy_or!(100))
            .build(scenario.ctx()),
    );
    let genesis_validator_addresses = validators.map_ref!(|v| v.sui_address());

    // create sui system state
    sui_system::create(
        object::new(scenario.ctx()), // it doesn't matter what ID sui system state has in tests
        validators,
        balance::create_for_testing<SUI>(storage_fund_amount.destroy_or!(0) * MIST_PER_SUI), // storage_fund
        protocol_version.destroy_or!(1), // protocol version
        0, // chain_start_timestamp_ms
        system_parameters,
        stake_subsidy,
        scenario.ctx(),
    );

    let mut runner = TestRunner {
        scenario,
        sender: @0,
        genesis_validator_addresses,
    };

    // set stake distribution counter if provided
    stake_distribution_counter.do!(|counter| {
        runner.system_tx!(|system, _| {
            system.set_stake_subsidy_distribution_counter(counter);
        });
    });

    // set start epoch if provided, useful for testing safe mode
    // TODO: what else could be configured for safe mode?
    start_epoch.do!(|epoch| {
        runner.scenario.skip_to_epoch(epoch);
        runner.system_tx!(|system, _| {
            system.set_epoch_for_testing(epoch);
        });
    });

    runner
}

public fun epoch_duration(mut builder: TestRunnerBuilder, epoch_duration: u64): TestRunnerBuilder {
    builder.epoch_duration = option::some(epoch_duration);
    builder
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

public fun start_epoch(mut builder: TestRunnerBuilder, start_epoch: u64): TestRunnerBuilder {
    builder.start_epoch = option::some(start_epoch);
    builder
}

public fun protocol_version(
    mut builder: TestRunnerBuilder,
    protocol_version: u64,
): TestRunnerBuilder {
    builder.protocol_version = option::some(protocol_version);
    builder
}

public fun stake_distribution_counter(
    mut builder: TestRunnerBuilder,
    stake_distribution_counter: u64,
): TestRunnerBuilder {
    builder.stake_distribution_counter = option::some(stake_distribution_counter);
    builder
}

// === Advance Epoch Options ===
public struct AdvanceEpochOptions has drop {
    protocol_version: Option<u64>,
    storage_charge: Option<u64>,
    computation_charge: Option<u64>,
    storage_rebate: Option<u64>,
    non_refundable_storage_fee: Option<u64>,
    storage_fund_reinvest_rate: Option<u64>,
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
        storage_fund_reinvest_rate: option::none(),
        reward_slashing_rate: option::none(),
        epoch_start_time: option::none(),
    }
}

public use fun protocol_version_opts as AdvanceEpochOptions.protocol_version;

public fun protocol_version_opts(
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

public fun storage_fund_reinvest_rate(
    mut opts: AdvanceEpochOptions,
    storage_fund_reinvest_rate: u64,
): AdvanceEpochOptions {
    opts.storage_fund_reinvest_rate = option::some(storage_fund_reinvest_rate);
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
    genesis_validator_addresses: vector<address>,
}

/// Set the sender of the next transaction.
public fun set_sender(runner: &mut TestRunner, sender: address): &mut TestRunner {
    runner.scenario.next_tx(sender);
    runner.sender = sender;
    runner
}

/// Get the current transaction context.
public fun ctx(runner: &mut TestRunner): &mut TxContext { runner.scenario.ctx() }

/// Get a mutable reference to the scenario.
public fun scenario_mut(runner: &mut TestRunner): &mut Scenario { &mut runner.scenario }

/// Get the initial validator addresses specified in the builder.
public fun genesis_validator_addresses(runner: &TestRunner): vector<address> {
    runner.genesis_validator_addresses
}

/// Keep an object in the sender's inventory.
public fun keep<T: key + store>(runner: &TestRunner, object: T) {
    transfer::public_transfer(object, runner.sender);
}

/// Get the sender of the next transaction.
public fun sender(runner: &mut TestRunner): address { runner.sender }

/// Mint a SUI balance for testing.
public fun mint(amount: u64): Balance<SUI> {
    balance::create_for_testing(amount * MIST_PER_SUI)
}

/// Destroy an object.
public fun destroy<T>(v: T) {
    std::unit_test::destroy(v);
}

/// Finish the test runner.
public fun finish(runner: TestRunner) {
    let TestRunner { scenario, .. } = runner;
    scenario.end();
}

// === Macros ===

/// Get a mutable reference to Scenario and call a function $f on it.
public macro fun scenario_fn($runner: &mut TestRunner, $f: |&mut Scenario|) {
    let sender = sender($runner);
    let scenario = scenario_mut($runner);
    scenario.next_tx(sender);
    $f(scenario);
}

/// Get an object from the sender's inventory and call a function $f on it.
public macro fun owned_tx<$Object>($runner: &mut TestRunner, $f: |$Object|) {
    let sender = sender($runner);
    let scenario = scenario_mut($runner);
    scenario.next_tx(sender);
    $f(scenario.take_from_sender<$Object>());
}

/// Run a transaction on the system state.
public macro fun system_tx(
    $runner: &mut TestRunner,
    $f: |&mut SuiSystemState, &mut TxContext|,
): &mut TestRunner {
    let sender = sender($runner);
    let scenario = scenario_mut($runner);
    scenario.next_tx(sender);
    let mut system_state = scenario.take_shared<SuiSystemState>();
    $f(&mut system_state, scenario.ctx());
    test_scenario::return_shared(system_state);
    $runner
}

/// Advance the epoch of the system state. Takes an optional `AdvanceEpochOptions` struct
/// to configure the epoch advance. Returns the storage rebate balance.
///
/// Switches to 0x0 for the sender of the `advance_epoch` transaction and then
/// switches back to the sender of the TestRunner.
///
/// ```rust
/// // default, no rewards
/// runner.advance_epoch(option::none()).destroy_for_testing();
///
/// // custom options, supports any combination of the following:
/// let opts = runner.advance_epoch_opts()
///     .storage_charge(100)
///     .computation_charge(200)
///     .storage_rebate(10)
///     .non_refundable_storage_fee(10)
///     .storage_fund_reinvest_rate(10)
///     .protocol_version(2)
///     .reward_slashing_rate(10);
///
/// runner.advance_epoch(option::some(opts)).destroy_for_testing();
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
        storage_fund_reinvest_rate,
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
                storage_fund_reinvest_rate.destroy_or!(0),
                reward_slashing_rate.destroy_or!(0),
                epoch_start_time.destroy_or!(0),
                ctx,
            );
    });

    runner.scenario.next_epoch(@0);
    runner.set_sender(sender);
    storage_rebate_balance
}

/// Simulates safe mode by changing the epoch value in the system object without
/// triggering the epoch change logic.
public fun advance_epoch_safe_mode(runner: &mut TestRunner) {
    let sender = runner.sender;
    runner.set_sender(@0);
    runner.system_tx!(|system, ctx| {
        system.set_epoch_for_testing(ctx.epoch() + 1);
    });
    runner.scenario.next_epoch(@0);
    runner.set_sender(sender);
}

/// Call the `request_add_stake` function on the system state.
public fun stake_with(runner: &mut TestRunner, validator: address, amount: u64) {
    let TestRunner { scenario, sender, .. } = runner;
    scenario.next_tx(*sender);
    runner.system_tx!(|system, ctx| {
        system.request_add_stake(
            coin::mint_for_testing(amount * MIST_PER_SUI, ctx),
            validator,
            ctx,
        );
    });
}

/// Call the `request_add_stake_non_entry` function on the system state.
public fun stake_with_and_take(
    runner: &mut TestRunner,
    validator: address,
    amount: u64,
): StakedSui {
    let TestRunner { scenario, sender, .. } = runner;
    let staked_sui;
    scenario.next_tx(*sender);
    runner.system_tx!(|system, ctx| {
        staked_sui =
            system.request_add_stake_non_entry(
                coin::mint_for_testing(amount * MIST_PER_SUI, ctx),
                validator,
                ctx,
            );
    });

    staked_sui
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

/// Report another validator as malicious.
public fun report_validator(runner: &mut TestRunner, validator: address) {
    runner.owned_tx!(|cap| {
        runner.system_tx!(|system, _| {
            system.report_validator(&cap, validator);
        });
        runner.keep(cap);
    });
}

/// Undo report a validator.
public fun undo_report_validator(runner: &mut TestRunner, validator: address) {
    runner.owned_tx!(|cap| {
        runner.system_tx!(|system, _| {
            system.undo_report_validator(&cap, validator);
        });
        runner.keep(cap);
    });
}

/// Set the reference gas price for the next epoch.
public fun set_gas_price(runner: &mut TestRunner, gas_price: u64) {
    runner.owned_tx!(|cap| {
        runner.system_tx!(|system, _| {
            system.request_set_gas_price(&cap, gas_price);
        });
        runner.keep(cap);
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

/// Get the sum of the StakedSui objects' principal and the rewards for these objects.
public fun staking_rewards_balance(runner: &mut TestRunner): u64 {
    let sender = runner.sender;
    let scenario = runner.scenario_mut();
    let mut system = scenario.take_shared<SuiSystemState>();

    scenario.next_tx(sender);
    let total_balance = scenario.ids_for_sender<StakedSui>().fold!(0, |mut sum, staked_sui_id| {
        let staked_sui = scenario.take_from_sender_by_id<StakedSui>(staked_sui_id);
        let rewards = system.calculate_rewards(&staked_sui, scenario.ctx());

        sum = sum + rewards + staked_sui.amount();
        scenario.return_to_sender(staked_sui);
        sum
    });

    test_scenario::return_shared(system);
    total_balance
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
