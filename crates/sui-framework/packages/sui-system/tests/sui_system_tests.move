// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This file contains tests testing functionalities in `sui_system` that are not
// already tested by the other more themed tests such as `stake_tests` or
// `rewards_distribution_tests`.

#[test_only]
module sui_system::sui_system_tests;

use std::unit_test::assert_eq;
use sui_system::test_runner;
use sui_system::validator_builder;
use sui_system::validator_cap::UnverifiedValidatorOperationCap;

const MIST_PER_SUI: u64 = 1_000_000_000;

#[test]
// Scenario: perform a series of report and undo report operations on a validator.
// Guarantees that:
// - report records are persisted across epochs.
// - report records are removed when a validator is removed.
// - report records are removed when a validator leaves.
// - duplicate report operations are ignored.
fun report_validator() {
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::new().sui_address(@1),
            validator_builder::new().sui_address(@2),
            validator_builder::new().sui_address(@3),
        ])
        .build();

    // Validator 1 reports validator 2
    runner.set_sender(@1).report_validator(@2);
    runner.system_tx!(|system, _| {
        assert_eq!(system.get_reporters_of(@2).into_keys(), vector[@1])
    });

    // Validator 3 reports validator 2
    runner.set_sender(@3).report_validator(@2);
    runner.system_tx!(|system, _| {
        assert_eq!(system.get_reporters_of(@2).into_keys(), vector[@1, @3])
    });

    // Report again and result should stay the same.
    runner.set_sender(@1).report_validator(@2);
    runner.system_tx!(|system, _| {
        assert_eq!(system.get_reporters_of(@2).into_keys(), vector[@1, @3])
    });

    // Undo the report from Validator 3.
    runner.set_sender(@3).undo_report_validator(@2);
    runner.system_tx!(|system, _| {
        assert_eq!(system.get_reporters_of(@2).into_keys(), vector[@1])
    });

    runner.advance_epoch(option::none()).destroy_for_testing();

    // After an epoch ends, report records are still present.
    runner.system_tx!(|system, _| {
        assert_eq!(system.get_reporters_of(@2).into_keys(), vector[@1])
    });

    // Validator 2 reports validator 1.
    runner.set_sender(@2).report_validator(@1);
    runner.system_tx!(|system, _| {
        assert_eq!(system.get_reporters_of(@1).into_keys(), vector[@2])
    });

    // Validator 3 reports validator 2 again.
    runner.set_sender(@3).report_validator(@2);
    runner.system_tx!(|system, _| {
        assert_eq!(system.get_reporters_of(@2).into_keys(), vector[@1, @3])
    });

    // After Validator 3 leaves, its reports are gone.
    runner.set_sender(@3).remove_validator();
    runner.advance_epoch(option::none()).destroy_for_testing();
    runner.system_tx!(|system, _| {
        assert_eq!(system.get_reporters_of(@2).into_keys(), vector[@1])
    });

    // Validator 1 leaves.
    runner.set_sender(@1).remove_validator();
    runner.advance_epoch(option::none()).destroy_for_testing();
    runner.system_tx!(|system, _| {
        assert!(system.get_reporters_of(@1).is_empty());
        assert!(system.get_reporters_of(@2).is_empty());
    });

    runner.finish();
}

#[random_test]
// Scenario: transfer the validator cap object to different addresses and check
//   that everything works as expected.
//
// TODO: discovered that pending validator does not set the gas price for the
//   first active epoch, but for an epoch after that. Confirm, that this is the
//   expected behavior.
fun report_validator_by_stakee_ok(initial_stake: u8) {
    let initial_stake = initial_stake.max(1) as u64;
    let mut runner = test_runner::new()
        .validators_initial_stake(initial_stake)
        .validators(vector[
            validator_builder::new().sui_address(@1),
            validator_builder::new().sui_address(@2),
        ])
        .build();

    let stakee = @0xbeef;

    // @0x1 transfers the cap object to stakee.
    runner.set_sender(@1).owned_tx!<UnverifiedValidatorOperationCap>(|cap| {
        transfer::public_transfer(cap, stakee);
    });

    // With the cap object in hand, stakee could report validators on behalf of @0x1.
    runner.set_sender(stakee).report_validator(@2);
    runner.system_tx!(|system, _| {
        assert_eq!(system.get_reporters_of(@2).into_keys(), vector[@1]);
    });

    // stakee could also undo report.
    runner.set_sender(stakee).undo_report_validator(@2);
    runner.system_tx!(|system, _| {
        assert!(system.get_reporters_of(@2).is_empty());
    });

    let new_stakee = @0xcafe;

    // transfer the cap object from `stakee` to `new_stakee`.
    runner.set_sender(stakee).owned_tx!<UnverifiedValidatorOperationCap>(|cap| {
        transfer::public_transfer(cap, new_stakee);
    });

    // `new_stakee` could report validators on behalf of @0x1.
    runner.set_sender(new_stakee).report_validator(@2);
    runner.system_tx!(|system, _| {
        assert_eq!(system.get_reporters_of(@2).into_keys(), vector[@1]);
    });

    // `new_stakee` could also set reference gas price on behalf of @0x1.
    runner.set_sender(new_stakee).set_gas_price(666);

    // Add a new pending validator
    runner.set_sender(@0);
    let validator = validator_builder::preset().initial_stake(initial_stake).build(runner.ctx());
    let new_validator = validator.sui_address();
    runner.add_validator_candidate(validator);
    runner.set_sender(new_validator).add_validator();

    // Pending validator could set reference price as well
    runner.set_sender(new_validator).set_gas_price(777);

    // Check that the next epoch gas price is set correctly
    runner.system_tx!(|system, _| {
        assert_eq!(system.active_validator_by_address(@1).next_epoch_gas_price(), 666);
        assert_eq!(system.pending_validator_by_address(new_validator).next_epoch_gas_price(), 777);
    });

    runner.advance_epoch(option::none()).destroy_for_testing();

    // Check that the next epoch gas price is set correctly
    runner.system_tx!(|system, _| {
        assert_eq!(system.active_validator_by_address(@1).gas_price(), 666);
        assert_eq!(system.active_validator_by_address(new_validator).gas_price(), 1);
    });

    runner.advance_epoch(option::none()).destroy_for_testing();

    // Pending validator's gas price is only accounted in the next epoch after it becoming active.
    runner.system_tx!(|system, _| {
        assert_eq!(system.active_validator_by_address(new_validator).gas_price(), 777);
    });

    runner.finish();
}

#[test, expected_failure(abort_code = ::sui_system::validator_set::EInvalidCap)]
fun report_validator_by_stakee_revoked() {
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::new().sui_address(@1),
            validator_builder::new().sui_address(@2),
        ])
        .build();

    // @0x1 transfers the cap object to stakee.
    let stakee = @0xbeef;
    runner.set_sender(@1).owned_tx!<UnverifiedValidatorOperationCap>(|cap| {
        transfer::public_transfer(cap, stakee);
    });

    // Confirm the stakee has permission to report validators.
    runner.set_sender(stakee).report_validator(@2);

    // Validator 1 revokes stakee's permission by creating a new cap object.
    runner.set_sender(@1).system_tx!(|system, ctx| {
        system.rotate_operation_cap(ctx);
    });

    // Stakee no longer has permission to report validators, here it aborts.
    runner.set_sender(stakee).undo_report_validator(@2);

    abort
}

#[test, expected_failure(abort_code = ::sui_system::validator_set::EInvalidCap)]
fun set_reference_gas_price_by_stakee_revoked() {
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::new().sui_address(@1),
            validator_builder::new().sui_address(@2),
        ])
        .build();

    // @0x1 transfers the cap object to stakee.
    let stakee = @0xbeef;
    runner.set_sender(@1).owned_tx!<UnverifiedValidatorOperationCap>(|cap| {
        transfer::public_transfer(cap, stakee);
    });

    // Confirm the stakee has permission to report validators.
    runner.set_sender(stakee).report_validator(@2);

    // Validator 1 revokes stakee's permission by creating a new cap object.
    runner.set_sender(@1).system_tx!(|system, ctx| {
        system.rotate_operation_cap(ctx);
    });

    // Stakee no longer has permission to set gas price, here it aborts.
    runner.set_sender(stakee).set_gas_price(888);

    abort
}

#[test, expected_failure(abort_code = sui_system::validator::EGasPriceHigherThanThreshold)]
fun set_gas_price_failure() {
    let validator = validator_builder::new().sui_address(@1);
    let mut runner = test_runner::new().validators(vector[validator]).build();

    // Fails here since the gas price is too high.
    runner.set_sender(@1).set_gas_price(100_001);

    abort
}

#[test, expected_failure(abort_code = sui_system::validator::ECommissionRateTooHigh)]
fun set_commission_rate_failure() {
    let validator = validator_builder::new().sui_address(@1);
    let mut runner = test_runner::new().validators(vector[validator]).build();

    // Fails here since the gas price is too high.
    runner.set_sender(@1).system_tx!(|system, ctx| {
        system.request_set_commission_rate(2001, ctx);
    });

    abort
}

#[test, expected_failure(abort_code = sui_system::sui_system_state_inner::ENotValidator)]
fun report_non_validator_failure() {
    let validator = validator_builder::new().sui_address(@1);
    let mut runner = test_runner::new().validators(vector[validator]).build();

    // Report a non-validator.
    runner.set_sender(@1).report_validator(@42);

    abort
}

#[test, expected_failure(abort_code = sui_system::sui_system_state_inner::EReportRecordNotFound)]
// TODO: the error expected here is not correct. Maybe we could improve this.
fun undo_report_non_validator_failure() {
    let validator = validator_builder::new().sui_address(@1);
    let mut runner = test_runner::new().validators(vector[validator]).build();

    // Undo a report on a non-validator.
    runner.set_sender(@1).undo_report_validator(@42);

    abort
}

#[test, expected_failure(abort_code = sui_system::sui_system_state_inner::ECannotReportOneself)]
fun report_self_failure() {
    let validator = validator_builder::new().sui_address(@1);
    let mut runner = test_runner::new().validators(vector[validator]).build();

    // Report oneself.
    runner.set_sender(@1).report_validator(@1);

    abort
}

#[test, expected_failure(abort_code = sui_system::sui_system_state_inner::EReportRecordNotFound)]
fun undo_report_failure() {
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::new().sui_address(@1),
            validator_builder::new().sui_address(@2),
        ])
        .build();

    // Undo a report that doesn't exist.
    runner.set_sender(@1).undo_report_validator(@2);

    abort
}

#[test]
fun validator_address_by_pool_id() {
    let validator = validator_builder::new().sui_address(@1);
    let mut runner = test_runner::new().validators(vector[validator]).build();

    runner.system_tx!(|system, _| {
        let pool_id = system.validator_staking_pool_id(@1);
        assert_eq!(system.validator_address_by_pool_id(&pool_id), @1);
    });

    runner.finish();
}

#[test]
fun staking_pool_mappings() {
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::new().sui_address(@1),
            validator_builder::new().sui_address(@2),
            validator_builder::new().sui_address(@3),
            validator_builder::new().sui_address(@4),
        ])
        .build();

    // Check that the pool mappings are correct.
    runner.system_tx!(|system, _| {
        let pool_id_1 = system.validator_staking_pool_id(@1);
        let pool_id_2 = system.validator_staking_pool_id(@2);
        let pool_id_3 = system.validator_staking_pool_id(@3);
        let pool_id_4 = system.validator_staking_pool_id(@4);
        let pool_mappings = system.validator_staking_pool_mappings();

        assert_eq!(pool_mappings.length(), 4);
        assert_eq!(pool_mappings[pool_id_1], @1);
        assert_eq!(pool_mappings[pool_id_2], @2);
        assert_eq!(pool_mappings[pool_id_3], @3);
        assert_eq!(pool_mappings[pool_id_4], @4);
    });

    // Add a new validator.
    runner.set_sender(@0);
    let validator = validator_builder::preset().initial_stake(100).build(runner.ctx());
    let new_validator = validator.sui_address();
    runner.add_validator_candidate(validator);
    runner.set_sender(new_validator).add_validator();
    runner.advance_epoch(option::none()).destroy_for_testing();

    // save this for later.
    let pool_id_1;

    // Check that the pool mappings are correct.
    runner.system_tx!(|system, _| {
        pool_id_1 = system.validator_staking_pool_id(@1);
        let pool_id_2 = system.validator_staking_pool_id(@2);
        let pool_id_3 = system.validator_staking_pool_id(@3);
        let pool_id_4 = system.validator_staking_pool_id(@4);
        let pool_id_5 = system.validator_staking_pool_id(new_validator);
        let pool_mappings = system.validator_staking_pool_mappings();

        assert_eq!(pool_mappings.length(), 5);
        assert_eq!(pool_mappings[pool_id_1], @1);
        assert_eq!(pool_mappings[pool_id_2], @2);
        assert_eq!(pool_mappings[pool_id_3], @3);
        assert_eq!(pool_mappings[pool_id_4], @4);
        assert_eq!(pool_mappings[pool_id_5], new_validator);
    });

    // Remove one of the original validators.
    runner.set_sender(@1).remove_validator();
    runner.advance_epoch(option::none()).destroy_for_testing();

    // Check pool mappings one last time. Validator 1 is expected to be removed.
    runner.system_tx!(|system, _| {
        let pool_id_2 = system.validator_staking_pool_id(@2);
        let pool_id_3 = system.validator_staking_pool_id(@3);
        let pool_id_4 = system.validator_staking_pool_id(@4);
        let pool_id_5 = system.validator_staking_pool_id(new_validator);
        let pool_mappings = system.validator_staking_pool_mappings();

        assert!(!pool_mappings.contains(pool_id_1));
        assert_eq!(pool_mappings.length(), 4);
        assert_eq!(pool_mappings[pool_id_2], @2);
        assert_eq!(pool_mappings[pool_id_3], @3);
        assert_eq!(pool_mappings[pool_id_4], @4);
        assert_eq!(pool_mappings[pool_id_5], new_validator);
    });

    runner.finish();
}

#[random_test]
/// Check that the stake subsidy distribution counter is incremented correctly,
/// depending on the configured epoch duration and the timestamp of the epoch start.
///
/// The test is parameterized by the epoch duration, which is chosen randomly.
fun skip_stake_subsidy(epoch_duration: u16) {
    let epoch_duration = epoch_duration as u64;
    let mut runner = test_runner::new()
        .epoch_duration(epoch_duration)
        .validators(vector[
            validator_builder::new().sui_address(@1),
            validator_builder::new().sui_address(@2),
        ])
        .build();

    // Advance epoch with the epoch duration timestamp.
    // Expect the counter to be incremented.
    let time = epoch_duration;
    let opts = runner.advance_epoch_opts().epoch_start_time(time);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();
    runner.system_tx!(|system, _| {
        let counter = system.get_stake_subsidy_distribution_counter();
        assert_eq!(counter, 1);
    });

    // Advance epoch with the epoch duration slightly less than the timestamp.
    // Expect the counter to not be incremented.
    let time = time + epoch_duration - 1;
    let opts = runner.advance_epoch_opts().epoch_start_time(time);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();
    runner.system_tx!(|system, _| {
        let counter = system.get_stake_subsidy_distribution_counter();
        assert_eq!(counter, 1);
    });

    // Advance epoch with the full epoch duration.
    // Expect the counter to be incremented.
    let time = time + epoch_duration;
    let opts = runner.advance_epoch_opts().epoch_start_time(time);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();
    runner.system_tx!(|system, _| {
        let counter = system.get_stake_subsidy_distribution_counter();
        assert_eq!(counter, 2);
    });

    runner.finish();
}

#[random_test]
// Stake random amount of SUI and check that the pending and withdraw amounts are correct.
fun withdraw_inactive_stake(stake: u16) {
    let stake_amount = stake as u64;
    let validator = validator_builder::new().sui_address(@1).initial_stake(100);
    let mut runner = test_runner::new().validators(vector[validator]).build();

    // Check initial staking values.
    runner.system_tx!(|system, _| {
        let pool = system.active_validator_by_address(@1).get_staking_pool_ref();
        assert_eq!(pool.pending_stake_amount(), 0);
        assert_eq!(pool.pending_stake_withdraw_amount(), 0);
        assert_eq!(pool.sui_balance(), 100 * MIST_PER_SUI);
    });

    // Stake 1 SUI.
    runner.set_sender(@5).stake_with(@1, stake_amount);

    // Check that pending stake amount is 1 SUI.
    runner.system_tx!(|system, _| {
        let pool = system.active_validator_by_address(@1).get_staking_pool_ref();
        assert_eq!(pool.pending_stake_amount(), stake_amount * MIST_PER_SUI);
        assert_eq!(pool.pending_stake_withdraw_amount(), 0);
        assert_eq!(pool.sui_balance(), 100 * MIST_PER_SUI);
    });

    // Unstake before activation epoch.
    runner.set_sender(@5).unstake(0);

    // Check that pending stake amount is 0.
    runner.system_tx!(|system, _| {
        let pool = system.active_validator_by_address(@1).get_staking_pool_ref();
        assert_eq!(pool.pending_stake_amount(), 0);
        assert_eq!(pool.pending_stake_withdraw_amount(), 0);
        assert_eq!(pool.sui_balance(), 100 * MIST_PER_SUI);
    });

    runner.finish();
}

#[random_test]
// Stake random amount of SUI and check that the pending stake amount is correct.
// Convert to fungible staked SUI and redeem it.
// Check that the stake amount is correct.
fun convert_to_fungible_staked_sui_and_redeem(stake: u16) {
    let stake_amount = stake as u64;
    let validator = validator_builder::new().sui_address(@1).initial_stake(100);
    let mut runner = test_runner::new().validators(vector[validator]).build();

    // Check initial stake values.
    runner.system_tx!(|system, _| {
        let pool = system.active_validator_by_address(@1).get_staking_pool_ref();
        assert_eq!(pool.pending_stake_amount(), 0);
        assert_eq!(pool.pending_stake_withdraw_amount(), 0);
        assert_eq!(pool.sui_balance(), 100 * MIST_PER_SUI);
    });

    let staked_sui = runner.set_sender(@5).stake_with_and_take(@1, stake_amount);

    assert_eq!(staked_sui.amount(), stake_amount * MIST_PER_SUI);

    // Stake is now active. Check that the stake amount is correct.
    runner.advance_epoch(option::none()).destroy_for_testing();
    runner.system_tx!(|system, _| {
        let pool = system.active_validator_by_address(@1).get_staking_pool_ref();
        assert_eq!(pool.pending_stake_amount(), 0);
        assert_eq!(pool.pending_stake_withdraw_amount(), 0);
        assert_eq!(pool.sui_balance(), (100 + stake_amount) * MIST_PER_SUI);
    });

    // Convert to fungible staked SUI.
    let fungible_staked_sui;
    runner.system_tx!(|system, ctx| {
        fungible_staked_sui = system.convert_to_fungible_staked_sui(staked_sui, ctx);
    });

    assert_eq!(fungible_staked_sui.value(), stake_amount * MIST_PER_SUI);

    let sui;
    runner.system_tx!(|system, ctx| {
        sui = system.redeem_fungible_staked_sui(fungible_staked_sui, ctx);
    });

    assert_eq!(sui.destroy_for_testing(), stake_amount * MIST_PER_SUI);

    runner.advance_epoch(option::none()).destroy_for_testing();
    runner.system_tx!(|system, _| {
        let pool = system.active_validator_by_address(@1).get_staking_pool_ref();
        assert_eq!(pool.pending_stake_amount(), 0);
        assert_eq!(pool.pending_stake_withdraw_amount(), 0);
        assert_eq!(pool.sui_balance(), 100 * MIST_PER_SUI);
    });

    runner.finish();
}
