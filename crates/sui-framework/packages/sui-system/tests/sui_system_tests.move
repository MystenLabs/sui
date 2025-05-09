// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This file contains tests testing functionalities in `sui_system` that are not
// already tested by the other more themed tests such as `stake_tests` or
// `rewards_distribution_tests`.

#[test_only]
module sui_system::sui_system_tests;

use std::unit_test::assert_eq;
use sui::balance;
use sui::coin;
use sui::sui::SUI;
use sui::test_scenario::{Self, Scenario};
use sui::test_utils::destroy;
use sui::url;
use sui_system::governance_test_utils::{
    advance_epoch,
    set_up_sui_system_state,
    create_sui_system_state_for_testing,
    stake_with,
    unstake
};
use sui_system::sui_system::SuiSystemState;
use sui_system::sui_system_state_inner;
use sui_system::test_runner;
use sui_system::validator::{Self, Validator};
use sui_system::validator_builder;
use sui_system::validator_cap::UnverifiedValidatorOperationCap;
use sui_system::validator_set;

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
            validator_builder::new().sui_address(@1).initial_stake(100),
            validator_builder::new().sui_address(@2).initial_stake(100),
            validator_builder::new().sui_address(@3).initial_stake(100),
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

#[test]
// Scenario: transfer the validator cap object to different addresses and check
//   that everything works as expected.
//
// TODO: discovered that pending validator does not set the gas price for the
//   first active epoch, but for an epoch after that. Confirm, that this is the
//   expected behavior.
fun report_validator_by_stakee_ok() {
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::new().sui_address(@1).initial_stake(100),
            validator_builder::new().sui_address(@2).initial_stake(100),
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
    let validator = validator_builder::preset().initial_stake(100).build(runner.ctx());
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
            validator_builder::new().sui_address(@1).initial_stake(100),
            validator_builder::new().sui_address(@2).initial_stake(100),
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
            validator_builder::new().sui_address(@1).initial_stake(100),
            validator_builder::new().sui_address(@2).initial_stake(100),
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

#[test, expected_failure(abort_code = validator::EGasPriceHigherThanThreshold)]
fun set_gas_price_failure() {
    let validator = validator_builder::new().sui_address(@1).initial_stake(100);
    let mut runner = test_runner::new().validators(vector[validator]).build();

    // Fails here since the gas price is too high.
    runner.set_sender(@1).set_gas_price(100_001);

    abort
}

#[test, expected_failure(abort_code = validator::ECommissionRateTooHigh)]
fun set_commission_rate_failure() {
    let validator = validator_builder::new().sui_address(@1).initial_stake(100);
    let mut runner = test_runner::new().validators(vector[validator]).build();

    // Fails here since the gas price is too high.
    runner.set_sender(@1).system_tx!(|system, ctx| {
        system.request_set_commission_rate(2001, ctx);
    });

    abort
}

#[test, expected_failure(abort_code = sui_system_state_inner::ENotValidator)]
fun report_non_validator_failure() {
    let validator = validator_builder::new().sui_address(@1).initial_stake(100);
    let mut runner = test_runner::new().validators(vector[validator]).build();

    // Report a non-validator.
    runner.set_sender(@1).report_validator(@42);

    abort
}

#[test, expected_failure(abort_code = sui_system_state_inner::EReportRecordNotFound)]
// TODO: the error expected here is not correct. Maybe we could improve this.
fun undo_report_non_validator_failure() {
    let validator = validator_builder::new().sui_address(@1).initial_stake(100);
    let mut runner = test_runner::new().validators(vector[validator]).build();

    // Undo a report on a non-validator.
    runner.set_sender(@1).undo_report_validator(@42);

    abort
}

#[test, expected_failure(abort_code = sui_system_state_inner::ECannotReportOneself)]
fun report_self_failure() {
    let validator = validator_builder::new().sui_address(@1).initial_stake(100);
    let mut runner = test_runner::new().validators(vector[validator]).build();

    // Report oneself.
    runner.set_sender(@1).report_validator(@1);

    abort
}

#[test, expected_failure(abort_code = sui_system_state_inner::EReportRecordNotFound)]
fun undo_report_failure() {
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::new().sui_address(@1).initial_stake(100),
            validator_builder::new().sui_address(@2).initial_stake(100),
        ])
        .build();

    // Undo a report that doesn't exist.
    runner.set_sender(@1).undo_report_validator(@2);

    abort
}

#[test]
fun validator_address_by_pool_id() {
    let validator = validator_builder::new().sui_address(@1).initial_stake(100);
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
            validator_builder::new().sui_address(@1).initial_stake(100),
            validator_builder::new().sui_address(@2).initial_stake(100),
            validator_builder::new().sui_address(@3).initial_stake(100),
            validator_builder::new().sui_address(@4).initial_stake(100),
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

#[test]
fun active_validator_update_metadata() {
    let validator_addr = @0xaf76afe6f866d8426d2be85d6ef0b11f871a251d043b2f11e15563bf418f5a5a;
    // pubkey generated with protocol key on seed [0; 32]
    let pubkey =
        x"99f25ef61f8032b914636460982c5cc6f134ef1ddae76657f2cbfec1ebfc8d097374080df6fcf0dcb8bc4b0d8e0af5d80ebbff2b4c599f54f42d6312dfc314276078c1cc347ebbbec5198be258513f386b930d02c2749a803e2330955ebd1a10";
    // pop generated using the protocol key and address with [fn test_proof_of_possession]
    let pop =
        x"b01cc86f421beca7ab4cfca87c0799c4d038c199dd399fbec1924d4d4367866dba9e84d514710b91feb65316e4ceef43";

    // pubkey generated with protocol key on seed [1; 32]
    let pubkey1 =
        x"96d19c53f1bee2158c3fcfb5bb2f06d3a8237667529d2d8f0fbb22fe5c3b3e64748420b4103674490476d98530d063271222d2a59b0f7932909cc455a30f00c69380e6885375e94243f7468e9563aad29330aca7ab431927540e9508888f0e1c";
    let pop1 =
        x"a8a0bcaf04e13565914eb22fa9f27a76f297db04446860ee2b923d10224cedb130b30783fb60b12556e7fc50e5b57a86";

    let new_validator_addr = @0x8e3446145b0c7768839d71840df389ffa3b9742d0baaff326a3d453b595f87d7;
    // pubkey generated with protocol key on seed [2; 32]
    let new_pubkey =
        x"adf2e2350fe9a58f3fa50777499f20331c4550ab70f6a4fb25a58c61b50b5366107b5c06332e71bb47aa99ce2d5c07fe0dab04b8af71589f0f292c50382eba6ad4c90acb010ab9db7412988b2aba1018aaf840b1390a8b2bee3fde35b4ab7fdf";
    let new_pop =
        x"926fdb08b2b46d802e3642044f215dcb049e6c17a376a272ffd7dba32739bb995370966698ab235ee172fbd974985cfe";

    // pubkey generated with protocol key on seed [3; 32]
    let new_pubkey1 =
        x"91b8de031e0b60861c655c8168596d98b065d57f26f287f8c810590b06a636eff13c4055983e95b2f60a4d6ba5484fa4176923d1f7807cc0b222ddf6179c1db099dba0433f098aae82542b3fd27b411d64a0a35aad01b2c07ac67f7d0a1d2c11";
    let new_pop1 =
        x"b61913eb4dc7ea1d92f174e1a3c6cad3f49ae8de40b13b69046ce072d8d778bfe87e734349c7394fd1543fff0cb6e2d0";

    let mut scenario_val = test_scenario::begin(validator_addr);
    let scenario = &mut scenario_val;

    // Set up SuiSystemState with an active validator
    let mut validators = vector[];
    let ctx = scenario.ctx();
    // prettier-ignore
    let validator = validator::new_for_testing(
        validator_addr,
        pubkey,
        vector[32, 219, 38, 23, 242, 109, 116, 235, 225, 192, 219, 45, 40, 124, 162, 25, 33, 68, 52, 41, 123, 9, 98, 11, 184, 150, 214, 62, 60, 210, 121, 62],
        vector[68, 55, 206, 25, 199, 14, 169, 53, 68, 92, 142, 136, 174, 149, 54, 215, 101, 63, 249, 206, 197, 98, 233, 80, 60, 12, 183, 32, 216, 88, 103, 25],
        pop,
        b"ValidatorName",
        b"description",
        b"image_url",
        b"project_url",
        b"/ip4/127.0.0.1/tcp/80",
        b"/ip4/127.0.0.1/udp/80",
        b"/ip4/127.0.0.1/udp/80",
        b"/ip4/127.0.0.1/udp/80",
        option::some(balance::create_for_testing<SUI>(100_000_000_000)),
        1,
        0,
        true,
        ctx,
    );
    validators.push_back(validator);
    create_sui_system_state_for_testing(validators, 1000, 0, ctx);

    scenario.next_tx(validator_addr);

    let mut system_state = scenario.take_shared<SuiSystemState>();

    // Test active validator metadata changes
    scenario.next_tx(validator_addr);
    {
        // prettier-ignore
        update_metadata(
            scenario,
            &mut system_state,
            b"validator_new_name",
            pubkey1,
            pop1,
            b"/ip4/42.42.42.42/tcp/80",
            b"/ip4/43.43.43.43/udp/80",
            vector[148, 117, 212, 171, 44, 104, 167, 11, 177, 100, 4, 55, 17, 235, 117, 45, 117, 84, 159, 49, 14, 159, 239, 246, 237, 21, 83, 166, 112, 53, 62, 199],
            vector[215, 64, 85, 185, 231, 116, 69, 151, 97, 79, 4, 183, 20, 70, 84, 51, 211, 162, 115, 221, 73, 241, 240, 171, 192, 25, 232, 106, 175, 162, 176, 43],
        );
    };

    scenario.next_tx(validator_addr);
    let validator = system_state.active_validator_by_address(validator_addr);

    // prettier-ignore
    verify_metadata(
        validator,
        b"validator_new_name",
        pubkey,
        pop,
        b"/ip4/127.0.0.1/tcp/80",
        b"/ip4/127.0.0.1/udp/80",
        vector[32, 219, 38, 23, 242, 109, 116, 235, 225, 192, 219, 45, 40, 124, 162, 25, 33, 68, 52, 41, 123, 9, 98, 11, 184, 150, 214, 62, 60, 210, 121, 62],
        vector[68, 55, 206, 25, 199, 14, 169, 53, 68, 92, 142, 136, 174, 149, 54, 215, 101, 63, 249, 206, 197, 98, 233, 80, 60, 12, 183, 32, 216, 88, 103, 25],
        pubkey1,
        pop1,
        b"/ip4/42.42.42.42/tcp/80",
        b"/ip4/43.43.43.43/udp/80",
        vector[148, 117, 212, 171, 44, 104, 167, 11, 177, 100, 4, 55, 17, 235, 117, 45, 117, 84, 159, 49, 14, 159, 239, 246, 237, 21, 83, 166, 112, 53, 62, 199],
        vector[215, 64, 85, 185, 231, 116, 69, 151, 97, 79, 4, 183, 20, 70, 84, 51, 211, 162, 115, 221, 73, 241, 240, 171, 192, 25, 232, 106, 175, 162, 176, 43],
    );

    test_scenario::return_shared(system_state);
    scenario_val.end();

    // Test pending validator metadata changes
    let mut scenario_val = test_scenario::begin(new_validator_addr);
    let scenario = &mut scenario_val;
    let mut system_state = scenario.take_shared<SuiSystemState>();
    scenario.next_tx(new_validator_addr);
    {
        let ctx = scenario.ctx();
        // prettier-ignore
        system_state.request_add_validator_candidate(
            new_pubkey,
            vector[33, 219, 38, 23, 242, 109, 116, 235, 225, 192, 219, 45, 40, 124, 162, 25, 33, 68, 52, 41, 123, 9, 98, 11, 184, 150, 214, 62, 60, 210, 121, 62],
            vector[69, 55, 206, 25, 199, 14, 169, 53, 68, 92, 142, 136, 174, 149, 54, 215, 101, 63, 249, 206, 197, 98, 233, 80, 60, 12, 183, 32, 216, 88, 103, 25],
            new_pop,
            b"ValidatorName2",
            b"description2",
            b"image_url2",
            b"project_url2",
            b"/ip4/127.0.0.2/tcp/80",
            b"/ip4/127.0.0.2/udp/80",
            b"/ip4/127.0.0.1/udp/80",
            b"/ip4/127.0.0.1/udp/80",
            1,
            0,
            ctx,
        );
        let staked_sui = system_state.request_add_stake_non_entry(
            coin::mint_for_testing(100_000_000_000, ctx),
            new_validator_addr,
            ctx,
        );
        transfer::public_transfer(staked_sui, @0x0);
        system_state.request_add_validator_for_testing(ctx);
    };

    scenario.next_tx(new_validator_addr);
    {
        // prettier-ignore
        update_metadata(
            scenario,
            &mut system_state,
            b"new_validator_new_name",
            new_pubkey1,
            new_pop1,
            b"/ip4/66.66.66.66/tcp/80",
            b"/ip4/77.77.77.77/udp/80",
            vector[215, 65, 85, 185, 231, 116, 69, 151, 97, 79, 4, 183, 20, 70, 84, 51, 211, 162, 115, 221, 73, 241, 240, 171, 192, 25, 232, 106, 175, 162, 176, 43],
            vector[149, 117, 212, 171, 44, 104, 167, 11, 177, 100, 4, 55, 17, 235, 117, 45, 117, 84, 159, 49, 14, 159, 239, 246, 237, 21, 83, 166, 112, 53, 62, 199],
        );
    };

    scenario.next_tx(new_validator_addr);
    let validator = system_state.pending_validator_by_address(new_validator_addr);
    // prettier-ignore
    verify_metadata(
        validator,
        b"new_validator_new_name",
        new_pubkey,
        new_pop,
        b"/ip4/127.0.0.2/tcp/80",
        b"/ip4/127.0.0.2/udp/80",
        vector[33, 219, 38, 23, 242, 109, 116, 235, 225, 192, 219, 45, 40, 124, 162, 25, 33, 68, 52, 41, 123, 9, 98, 11, 184, 150, 214, 62, 60, 210, 121, 62],
        vector[69, 55, 206, 25, 199, 14, 169, 53, 68, 92, 142, 136, 174, 149, 54, 215, 101, 63, 249, 206, 197, 98, 233, 80, 60, 12, 183, 32, 216, 88, 103, 25],
        new_pubkey1,
        new_pop1,
        b"/ip4/66.66.66.66/tcp/80",
        b"/ip4/77.77.77.77/udp/80",
        vector[215, 65, 85, 185, 231, 116, 69, 151, 97, 79, 4, 183, 20, 70, 84, 51, 211, 162, 115, 221, 73, 241, 240, 171, 192, 25, 232, 106, 175, 162, 176, 43],
        vector[149, 117, 212, 171, 44, 104, 167, 11, 177, 100, 4, 55, 17, 235, 117, 45, 117, 84, 159, 49, 14, 159, 239, 246, 237, 21, 83, 166, 112, 53, 62, 199],
    );

    test_scenario::return_shared(system_state);

    // Advance epoch to effectuate the metadata changes.
    scenario.next_tx(new_validator_addr);
    advance_epoch(scenario);

    // Now both validators are active, verify their metadata.
    scenario.next_tx(new_validator_addr);
    let mut system_state = scenario.take_shared<SuiSystemState>();
    let validator = system_state.active_validator_by_address(validator_addr);
    // prettier-ignore
    verify_metadata_after_advancing_epoch(
        validator,
        b"validator_new_name",
        pubkey1,
        pop1,
        b"/ip4/42.42.42.42/tcp/80",
        b"/ip4/43.43.43.43/udp/80",
        vector[148, 117, 212, 171, 44, 104, 167, 11, 177, 100, 4, 55, 17, 235, 117, 45, 117, 84, 159, 49, 14, 159, 239, 246, 237, 21, 83, 166, 112, 53, 62, 199],
        vector[215, 64, 85, 185, 231, 116, 69, 151, 97, 79, 4, 183, 20, 70, 84, 51, 211, 162, 115, 221, 73, 241, 240, 171, 192, 25, 232, 106, 175, 162, 176, 43],
    );

    let validator = system_state.active_validator_by_address(new_validator_addr);
    // prettier-ignore
    verify_metadata_after_advancing_epoch(
        validator,
        b"new_validator_new_name",
        new_pubkey1,
        new_pop1,
        b"/ip4/66.66.66.66/tcp/80",
        b"/ip4/77.77.77.77/udp/80",
        vector[215, 65, 85, 185, 231, 116, 69, 151, 97, 79, 4, 183, 20, 70, 84, 51, 211, 162, 115, 221, 73, 241, 240, 171, 192, 25, 232, 106, 175, 162, 176, 43],
        vector[149, 117, 212, 171, 44, 104, 167, 11, 177, 100, 4, 55, 17, 235, 117, 45, 117, 84, 159, 49, 14, 159, 239, 246, 237, 21, 83, 166, 112, 53, 62, 199],
    );

    test_scenario::return_shared(system_state);
    scenario_val.end();
}

#[test]
fun validator_candidate_update() {
    let validator_addr = @0xaf76afe6f866d8426d2be85d6ef0b11f871a251d043b2f11e15563bf418f5a5a;
    // pubkey generated with protocol key on seed [0; 32]
    let pubkey =
        x"99f25ef61f8032b914636460982c5cc6f134ef1ddae76657f2cbfec1ebfc8d097374080df6fcf0dcb8bc4b0d8e0af5d80ebbff2b4c599f54f42d6312dfc314276078c1cc347ebbbec5198be258513f386b930d02c2749a803e2330955ebd1a10";
    // pop generated using the protocol key and address with [fn test_proof_of_possession]
    let pop =
        x"b01cc86f421beca7ab4cfca87c0799c4d038c199dd399fbec1924d4d4367866dba9e84d514710b91feb65316e4ceef43";

    // pubkey generated with protocol key on seed [1; 32]
    let pubkey1 =
        x"96d19c53f1bee2158c3fcfb5bb2f06d3a8237667529d2d8f0fbb22fe5c3b3e64748420b4103674490476d98530d063271222d2a59b0f7932909cc455a30f00c69380e6885375e94243f7468e9563aad29330aca7ab431927540e9508888f0e1c";
    let pop1 =
        x"a8a0bcaf04e13565914eb22fa9f27a76f297db04446860ee2b923d10224cedb130b30783fb60b12556e7fc50e5b57a86";

    let mut scenario_val = test_scenario::begin(validator_addr);
    let scenario = &mut scenario_val;

    set_up_sui_system_state(vector[@0x1, @0x2, @0x3]);
    scenario.next_tx(validator_addr);
    let mut system_state = scenario.take_shared<SuiSystemState>();
    scenario.next_tx(validator_addr);
    {
        // prettier-ignore
        system_state.request_add_validator_candidate_for_testing(
            pubkey,
            vector[215, 64, 85, 185, 231, 116, 69, 151, 97, 79, 4, 183, 20, 70, 84, 51, 211, 162, 115, 221, 73, 241, 240, 171, 192, 25, 232, 106, 175, 162, 176, 43],
            vector[148, 117, 212, 171, 44, 104, 167, 11, 177, 100, 4, 55, 17, 235, 117, 45, 117, 84, 159, 49, 14, 159, 239, 246, 237, 21, 83, 166, 112, 53, 62, 199],
            pop,
            b"ValidatorName2",
            b"description2",
            b"image_url2",
            b"project_url2",
            b"/ip4/127.0.0.2/tcp/80",
            b"/ip4/127.0.0.2/udp/80",
            b"/ip4/168.168.168.168/udp/80",
            b"/ip4/168.168.168.168/udp/80",
            1,
            0,
            scenario.ctx(),
        );
    };

    scenario.next_tx(validator_addr);
    update_candidate(
        scenario,
        &mut system_state,
        b"validator_new_name",
        pubkey1,
        pop1,
        b"/ip4/42.42.42.42/tcp/80",
        b"/ip4/43.43.43.43/udp/80",
        42,
        7,
    );

    scenario.next_tx(validator_addr);

    let validator = system_state.candidate_validator_by_address(validator_addr);
    verify_candidate(
        validator,
        b"validator_new_name",
        pubkey1,
        pop1,
        b"/ip4/42.42.42.42/tcp/80",
        b"/ip4/43.43.43.43/udp/80",
        42,
        7,
    );

    test_scenario::return_shared(system_state);
    scenario_val.end();
}

#[test, expected_failure(abort_code = validator::EMetadataInvalidWorkerPubkey)]
fun add_validator_candidate_failure_invalid_metadata() {
    let mut scenario_val = test_scenario::begin(@0x0);
    let scenario = &mut scenario_val;

    // Generated using [fn test_proof_of_possession]
    let new_validator_addr = @0x8e3446145b0c7768839d71840df389ffa3b9742d0baaff326a3d453b595f87d7;
    let pubkey =
        x"99f25ef61f8032b914636460982c5cc6f134ef1ddae76657f2cbfec1ebfc8d097374080df6fcf0dcb8bc4b0d8e0af5d80ebbff2b4c599f54f42d6312dfc314276078c1cc347ebbbec5198be258513f386b930d02c2749a803e2330955ebd1a10";
    let pop =
        x"83809369ce6572be211512d85621a075ee6a8da57fbb2d867d05e6a395e71f10e4e957796944d68a051381eb91720fba";

    set_up_sui_system_state(vector[@0x1, @0x2, @0x3]);
    scenario.next_tx(new_validator_addr);
    let mut system_state = scenario.take_shared<SuiSystemState>();
    // prettier-ignore
    system_state.request_add_validator_candidate(
        pubkey,
        vector[32, 219, 38, 23, 242, 109, 116, 235, 225, 192, 219, 45, 40, 124, 162, 25, 33, 68, 52, 41, 123, 9, 98, 11, 184, 150, 214, 62, 60, 210, 121, 62],
        vector[42], // invalid
        pop,
        b"ValidatorName2",
        b"description2",
        b"image_url2",
        b"project_url2",
        b"/ip4/127.0.0.2/tcp/80",
        b"/ip4/127.0.0.2/udp/80",
        b"/ip4/127.0.0.1/udp/80",
        b"/ip4/127.0.0.1/udp/80",
        1,
        0,
        scenario.ctx(),
    );
    test_scenario::return_shared(system_state);
    scenario_val.end();
}

#[test, expected_failure(abort_code = validator_set::EAlreadyValidatorCandidate)]
fun add_validator_candidate_failure_double_register() {
    let mut scenario_val = test_scenario::begin(@0x0);
    let scenario = &mut scenario_val;
    let new_validator_addr = @0x8e3446145b0c7768839d71840df389ffa3b9742d0baaff326a3d453b595f87d7;
    let pubkey =
        x"99f25ef61f8032b914636460982c5cc6f134ef1ddae76657f2cbfec1ebfc8d097374080df6fcf0dcb8bc4b0d8e0af5d80ebbff2b4c599f54f42d6312dfc314276078c1cc347ebbbec5198be258513f386b930d02c2749a803e2330955ebd1a10";
    let pop =
        x"83809369ce6572be211512d85621a075ee6a8da57fbb2d867d05e6a395e71f10e4e957796944d68a051381eb91720fba";

    set_up_sui_system_state(vector[@0x1, @0x2, @0x3]);
    scenario.next_tx(new_validator_addr);
    let mut system_state = scenario.take_shared<SuiSystemState>();
    // prettier-ignore
    system_state.request_add_validator_candidate(
        pubkey,
        vector[32, 219, 38, 23, 242, 109, 116, 235, 225, 192, 219, 45, 40, 124, 162, 25, 33, 68, 52, 41, 123, 9, 98, 11, 184, 150, 214, 62, 60, 210, 121, 62],
        vector[68, 55, 206, 25, 199, 14, 169, 53, 68, 92, 142, 136, 174, 149, 54, 215, 101, 63, 249, 206, 197, 98, 233, 80, 60, 12, 183, 32, 216, 88, 103, 25],
        pop,
        b"ValidatorName2",
        b"description2",
        b"image_url2",
        b"project_url2",
        b"/ip4/127.0.0.2/tcp/80",
        b"/ip4/127.0.0.2/udp/80",
        b"/ip4/127.0.0.1/udp/80",
        b"/ip4/127.0.0.1/udp/80",
        1,
        0,
        scenario.ctx(),
    );

    // prettier-ignore
    // Add the same address as candidate again, should fail this time.
    system_state.request_add_validator_candidate(
        pubkey,
        vector[32, 219, 38, 23, 242, 109, 116, 235, 225, 192, 219, 45, 40, 124, 162, 25, 33, 68, 52, 41, 123, 9, 98, 11, 184, 150, 214, 62, 60, 210, 121, 62],
        vector[68, 55, 206, 25, 199, 14, 169, 53, 68, 92, 142, 136, 174, 149, 54, 215, 101, 63, 249, 206, 197, 98, 233, 80, 60, 12, 183, 32, 216, 88, 103, 25],
        pop,
        b"ValidatorName2",
        b"description2",
        b"image_url2",
        b"project_url2",
        b"/ip4/127.0.0.2/tcp/80",
        b"/ip4/127.0.0.2/udp/80",
        b"/ip4/127.0.0.1/udp/80",
        b"/ip4/127.0.0.1/udp/80",
        1,
        0,
        scenario.ctx(),
    );
    test_scenario::return_shared(system_state);
    scenario_val.end();
}

#[test, expected_failure(abort_code = validator_set::EDuplicateValidator)]
fun add_validator_candidate_failure_duplicate_with_active() {
    let validator_addr = @0xaf76afe6f866d8426d2be85d6ef0b11f871a251d043b2f11e15563bf418f5a5a;
    // Seed [0; 32]
    let pubkey =
        x"99f25ef61f8032b914636460982c5cc6f134ef1ddae76657f2cbfec1ebfc8d097374080df6fcf0dcb8bc4b0d8e0af5d80ebbff2b4c599f54f42d6312dfc314276078c1cc347ebbbec5198be258513f386b930d02c2749a803e2330955ebd1a10";
    let pop =
        x"b01cc86f421beca7ab4cfca87c0799c4d038c199dd399fbec1924d4d4367866dba9e84d514710b91feb65316e4ceef43";

    let new_addr = @0x1a4623343cd42be47d67314fce0ad042f3c82685544bc91d8c11d24e74ba7357;
    // Seed [1; 32]
    let new_pubkey =
        x"96d19c53f1bee2158c3fcfb5bb2f06d3a8237667529d2d8f0fbb22fe5c3b3e64748420b4103674490476d98530d063271222d2a59b0f7932909cc455a30f00c69380e6885375e94243f7468e9563aad29330aca7ab431927540e9508888f0e1c";
    let new_pop =
        x"932336c35a8c393019c63eb0f7d385dd4e0bd131f04b54cf45aa9544f14dca4dab53bd70ffcb8e0b34656e4388309720";

    let mut scenario_val = test_scenario::begin(validator_addr);
    let scenario = &mut scenario_val;

    // Set up SuiSystemState with an active validator
    let ctx = scenario.ctx();
    // prettier-ignore
    let validator = validator::new_for_testing(
        validator_addr,
        pubkey,
        vector[32, 219, 38, 23, 242, 109, 116, 235, 225, 192, 219, 45, 40, 124, 162, 25, 33, 68, 52, 41, 123, 9, 98, 11, 184, 150, 214, 62, 60, 210, 121, 62],
        vector[68, 55, 206, 25, 199, 14, 169, 53, 68, 92, 142, 136, 174, 149, 54, 215, 101, 63, 249, 206, 197, 98, 233, 80, 60, 12, 183, 32, 216, 88, 103, 25],
        pop,
        b"ValidatorName",
        b"description",
        b"image_url",
        b"project_url",
        b"/ip4/127.0.0.1/tcp/80",
        b"/ip4/127.0.0.1/udp/80",
        b"/ip4/127.0.0.1/udp/80",
        b"/ip4/127.0.0.1/udp/80",
        option::some(balance::create_for_testing<SUI>(100_000_000_000)),
        1,
        0,
        true,
        ctx,
    );
    create_sui_system_state_for_testing(vector[validator], 1000, 0, ctx);

    scenario.next_tx(new_addr);

    let mut system_state = scenario.take_shared<SuiSystemState>();

    // prettier-ignore
    // Add a candidate with the same name. Fails due to duplicating with an already active validator.
    system_state.request_add_validator_candidate(
        new_pubkey,
        vector[115, 220, 238, 151, 134, 159, 173, 41, 80, 2, 66, 196, 61, 17, 191, 76, 103, 39, 246, 127, 171, 85, 19, 235, 210, 106, 97, 97, 116, 48, 244, 191],
        vector[149, 128, 161, 13, 11, 183, 96, 45, 89, 20, 188, 205, 26, 127, 147, 254, 184, 229, 184, 102, 64, 170, 104, 29, 191, 171, 91, 99, 58, 178, 41, 156],
        new_pop,
        // same name
        b"ValidatorName",
        b"description2",
        b"image_url2",
        b"project_url2",
        b"/ip4/127.0.0.2/tcp/80",
        b"/ip4/127.0.0.2/udp/80",
        b"/ip4/127.0.0.1/udp/80",
        b"/ip4/127.0.0.1/udp/80",
        1,
        0,
        scenario.ctx(),
    );
    test_scenario::return_shared(system_state);
    scenario_val.end();
}

#[test]
fun skip_stake_subsidy() {
    let mut scenario_val = test_scenario::begin(@0x0);
    let scenario = &mut scenario_val;
    // Epoch duration is set to be 42 here.
    set_up_sui_system_state(vector[@0x1, @0x2]);

    // If the epoch length is less than 42 then the stake subsidy distribution counter should not be incremented. Otherwise it should.
    advance_epoch_and_check_distribution_counter(scenario, 42, true);
    advance_epoch_and_check_distribution_counter(scenario, 32, false);
    advance_epoch_and_check_distribution_counter(scenario, 52, true);
    scenario_val.end();
}

fun advance_epoch_and_check_distribution_counter(
    scenario: &mut Scenario,
    epoch_length: u64,
    should_increment_counter: bool,
) {
    scenario.next_tx(@0x0);
    let new_epoch = scenario.ctx().epoch() + 1;
    let mut system_state = scenario.take_shared<SuiSystemState>();
    let prev_epoch_time = system_state.epoch_start_timestamp_ms();
    let prev_counter = system_state.get_stake_subsidy_distribution_counter();

    let rebate = system_state.advance_epoch_for_testing(
        new_epoch,
        1,
        0,
        0,
        0,
        0,
        0,
        0,
        prev_epoch_time + epoch_length,
        scenario.ctx(),
    );
    destroy(rebate);
    assert_eq!(
        system_state.get_stake_subsidy_distribution_counter(),
        prev_counter + (if (should_increment_counter) 1 else 0),
    );
    test_scenario::return_shared(system_state);
    scenario.next_epoch(@0x0);
}

#[test]
fun withdraw_inactive_stake() {
    let mut scenario_val = test_scenario::begin(@0x0);
    let scenario = &mut scenario_val;
    // Epoch duration is set to be 42 here.
    set_up_sui_system_state(vector[@0x1, @0x2]);

    {
        scenario.next_tx(@0x0);
        let mut system_state = scenario.take_shared<SuiSystemState>();
        let staking_pool = system_state.active_validator_by_address(@0x1).get_staking_pool_ref();

        assert!(staking_pool.pending_stake_amount() == 0, 0);
        assert!(staking_pool.pending_stake_withdraw_amount() == 0, 0);
        assert!(staking_pool.sui_balance() == 100 * 1_000_000_000, 0);

        test_scenario::return_shared(system_state);
    };

    stake_with(@0x0, @0x1, 1, scenario);

    {
        scenario.next_tx(@0x0);
        let mut system_state = scenario.take_shared<SuiSystemState>();
        let staking_pool = system_state.active_validator_by_address(@0x1).get_staking_pool_ref();

        assert!(staking_pool.pending_stake_amount() == 1_000_000_000, 0);
        assert!(staking_pool.pending_stake_withdraw_amount() == 0, 0);
        assert!(staking_pool.sui_balance() == 100 * 1_000_000_000, 0);

        test_scenario::return_shared(system_state);
    };

    unstake(@0x0, 0, scenario);

    {
        scenario.next_tx(@0x0);
        let mut system_state = scenario.take_shared<SuiSystemState>();
        let staking_pool = system_state.active_validator_by_address(@0x1).get_staking_pool_ref();

        assert!(staking_pool.pending_stake_amount() == 0, 0);
        assert!(staking_pool.pending_stake_withdraw_amount() == 0, 0);
        assert!(staking_pool.sui_balance() == 100 * 1_000_000_000, 0);

        test_scenario::return_shared(system_state);
    };

    scenario_val.end();
}

#[test]
fun convert_to_fungible_staked_sui_and_redeem() {
    let mut scenario_val = test_scenario::begin(@0x0);
    let scenario = &mut scenario_val;
    // Epoch duration is set to be 42 here.
    set_up_sui_system_state(vector[@0x1, @0x2]);

    {
        scenario.next_tx(@0x0);
        let mut system_state = scenario.take_shared<SuiSystemState>();
        let staking_pool = system_state.active_validator_by_address(@0x1).get_staking_pool_ref();

        assert!(staking_pool.pending_stake_amount() == 0, 0);
        assert!(staking_pool.pending_stake_withdraw_amount() == 0, 0);
        assert!(staking_pool.sui_balance() == 100 * 1_000_000_000, 0);

        test_scenario::return_shared(system_state);
    };

    scenario.next_tx(@0x0);
    let mut system_state = scenario.take_shared<SuiSystemState>();

    let staked_sui = system_state.request_add_stake_non_entry(
        coin::mint_for_testing(100_000_000_000, scenario.ctx()),
        @0x1,
        scenario.ctx(),
    );

    assert!(staked_sui.amount() == 100_000_000_000, 0);

    test_scenario::return_shared(system_state);
    advance_epoch(scenario);

    let mut system_state = scenario.take_shared<SuiSystemState>();
    let fungible_staked_sui = system_state.convert_to_fungible_staked_sui(
        staked_sui,
        scenario.ctx(),
    );

    assert!(fungible_staked_sui.value() == 100_000_000_000, 0);

    let sui = system_state.redeem_fungible_staked_sui(
        fungible_staked_sui,
        scenario.ctx(),
    );

    assert!(sui.value() == 100_000_000_000, 0);

    test_scenario::return_shared(system_state);

    advance_epoch(scenario);

    sui::test_utils::destroy(sui);
    scenario_val.end();
}

fun update_candidate(
    scenario: &mut Scenario,
    system_state: &mut SuiSystemState,
    name: vector<u8>,
    protocol_pub_key: vector<u8>,
    pop: vector<u8>,
    network_address: vector<u8>,
    p2p_address: vector<u8>,
    commission_rate: u64,
    gas_price: u64,
) {
    let ctx = scenario.ctx();
    system_state.update_validator_name(name, ctx);
    system_state.update_validator_description(b"new_desc", ctx);
    system_state.update_validator_image_url(b"new_image_url", ctx);
    system_state.update_validator_project_url(b"new_project_url", ctx);
    system_state.update_candidate_validator_network_address(network_address, ctx);
    system_state.update_candidate_validator_p2p_address(p2p_address, ctx);
    system_state.update_candidate_validator_primary_address(b"/ip4/127.0.0.1/udp/80", ctx);
    system_state.update_candidate_validator_worker_address(b"/ip4/127.0.0.1/udp/80", ctx);
    system_state.update_candidate_validator_protocol_pubkey(
        protocol_pub_key,
        pop,
        ctx,
    );

    // prettier-ignore
    system_state.update_candidate_validator_worker_pubkey(
        vector[68, 55, 206, 25, 199, 14, 169, 53, 68, 92, 142, 136, 174, 149, 54, 215, 101, 63, 249, 206, 197, 98, 233, 80, 60, 12, 183, 32, 216, 88, 103, 25],
        ctx,
    );
    // prettier-ignore
    system_state.update_candidate_validator_network_pubkey(
        vector[32, 219, 38, 23, 242, 109, 116, 235, 225, 192, 219, 45, 40, 124, 162, 25, 33, 68, 52, 41, 123, 9, 98, 11, 184, 150, 214, 62, 60, 210, 121, 62],
        ctx,
    );

    system_state.set_candidate_validator_commission_rate(commission_rate, ctx);
    let cap = scenario.take_from_sender<UnverifiedValidatorOperationCap>();
    system_state.set_candidate_validator_gas_price(&cap, gas_price);
    scenario.return_to_sender(cap);
}

fun verify_candidate(
    validator: &Validator,
    name: vector<u8>,
    protocol_pub_key: vector<u8>,
    pop: vector<u8>,
    network_address: vector<u8>,
    p2p_address: vector<u8>,
    commission_rate: u64,
    gas_price: u64,
) {
    // prettier-ignore
    verify_current_epoch_metadata(
        validator,
        name,
        protocol_pub_key,
        pop,
        b"/ip4/127.0.0.1/udp/80",
        b"/ip4/127.0.0.1/udp/80",
        network_address,
        p2p_address,
        vector[32, 219, 38, 23, 242, 109, 116, 235, 225, 192, 219, 45, 40, 124, 162, 25, 33, 68, 52, 41, 123, 9, 98, 11, 184, 150, 214, 62, 60, 210, 121, 62],
        vector[68, 55, 206, 25, 199, 14, 169, 53, 68, 92, 142, 136, 174, 149, 54, 215, 101, 63, 249, 206, 197, 98, 233, 80, 60, 12, 183, 32, 216, 88, 103, 25],
    );
    assert!(validator.commission_rate() == commission_rate);
    assert!(validator.gas_price() == gas_price);
}

// Note: `pop` MUST be a valid signature using sui_address and protocol_pubkey_bytes.
// To produce a valid PoP, run [fn test_proof_of_possession].
fun update_metadata(
    scenario: &mut Scenario,
    system_state: &mut SuiSystemState,
    name: vector<u8>,
    protocol_pub_key: vector<u8>,
    pop: vector<u8>,
    network_address: vector<u8>,
    p2p_address: vector<u8>,
    network_pubkey: vector<u8>,
    worker_pubkey: vector<u8>,
) {
    let ctx = scenario.ctx();
    system_state.update_validator_name(name, ctx);
    system_state.update_validator_description(b"new_desc", ctx);
    system_state.update_validator_image_url(b"new_image_url", ctx);
    system_state.update_validator_project_url(b"new_project_url", ctx);
    system_state.update_validator_next_epoch_network_address(network_address, ctx);
    system_state.update_validator_next_epoch_p2p_address(p2p_address, ctx);
    system_state.update_validator_next_epoch_primary_address(b"/ip4/168.168.168.168/udp/80", ctx);
    system_state.update_validator_next_epoch_worker_address(b"/ip4/168.168.168.168/udp/80", ctx);
    system_state.update_validator_next_epoch_protocol_pubkey(
        protocol_pub_key,
        pop,
        ctx,
    );
    system_state.update_validator_next_epoch_network_pubkey(network_pubkey, ctx);
    system_state.update_validator_next_epoch_worker_pubkey(worker_pubkey, ctx);
}

fun verify_metadata(
    validator: &Validator,
    name: vector<u8>,
    protocol_pub_key: vector<u8>,
    pop: vector<u8>,
    network_address: vector<u8>,
    p2p_address: vector<u8>,
    network_pubkey: vector<u8>,
    worker_pubkey: vector<u8>,
    new_protocol_pub_key: vector<u8>,
    new_pop: vector<u8>,
    new_network_address: vector<u8>,
    new_p2p_address: vector<u8>,
    new_network_pubkey: vector<u8>,
    new_worker_pubkey: vector<u8>,
) {
    // Current epoch
    verify_current_epoch_metadata(
        validator,
        name,
        protocol_pub_key,
        pop,
        b"/ip4/127.0.0.1/udp/80",
        b"/ip4/127.0.0.1/udp/80",
        network_address,
        p2p_address,
        network_pubkey,
        worker_pubkey,
    );

    // Next epoch
    assert!(
        validator.next_epoch_network_address() == &option::some(new_network_address.to_string()),
    );
    assert!(validator.next_epoch_p2p_address() == &option::some(new_p2p_address.to_string()));
    assert!(
        validator.next_epoch_primary_address() == &option::some(b"/ip4/168.168.168.168/udp/80".to_string()),
    );
    assert!(
        validator.next_epoch_worker_address() == &option::some(b"/ip4/168.168.168.168/udp/80".to_string()),
    );
    assert!(validator.next_epoch_protocol_pubkey_bytes() == &option::some(new_protocol_pub_key), 0);
    assert!(validator.next_epoch_proof_of_possession() == &option::some(new_pop), 0);
    assert!(validator.next_epoch_worker_pubkey_bytes() == &option::some(new_worker_pubkey), 0);
    assert!(validator.next_epoch_network_pubkey_bytes() == &option::some(new_network_pubkey), 0);
}

fun verify_current_epoch_metadata(
    validator: &Validator,
    name: vector<u8>,
    protocol_pub_key: vector<u8>,
    pop: vector<u8>,
    primary_address: vector<u8>,
    worker_address: vector<u8>,
    network_address: vector<u8>,
    p2p_address: vector<u8>,
    network_pubkey_bytes: vector<u8>,
    worker_pubkey_bytes: vector<u8>,
) {
    // Current epoch
    assert!(validator.name() == &name.to_string());
    assert!(validator.description() == &b"new_desc".to_string());
    assert!(validator.image_url() == &url::new_unsafe_from_bytes(b"new_image_url"));
    assert!(validator.project_url() == &url::new_unsafe_from_bytes(b"new_project_url"));
    assert!(validator.network_address() == &network_address.to_string());
    assert!(validator.p2p_address() == &p2p_address.to_string());
    assert!(validator.primary_address() == &primary_address.to_string());
    assert!(validator.worker_address() == &worker_address.to_string());
    assert!(validator.protocol_pubkey_bytes() == &protocol_pub_key);
    assert!(validator.proof_of_possession() == &pop);
    assert!(validator.worker_pubkey_bytes() == &worker_pubkey_bytes);
    assert!(validator.network_pubkey_bytes() == &network_pubkey_bytes);
}

fun verify_metadata_after_advancing_epoch(
    validator: &Validator,
    name: vector<u8>,
    protocol_pub_key: vector<u8>,
    pop: vector<u8>,
    network_address: vector<u8>,
    p2p_address: vector<u8>,
    network_pubkey: vector<u8>,
    worker_pubkey: vector<u8>,
) {
    // Current epoch
    verify_current_epoch_metadata(
        validator,
        name,
        protocol_pub_key,
        pop,
        b"/ip4/168.168.168.168/udp/80",
        b"/ip4/168.168.168.168/udp/80",
        network_address,
        p2p_address,
        network_pubkey,
        worker_pubkey,
    );

    // Next epoch
    assert!(validator.next_epoch_network_address().is_none());
    assert!(validator.next_epoch_p2p_address().is_none());
    assert!(validator.next_epoch_primary_address().is_none());
    assert!(validator.next_epoch_worker_address().is_none());
    assert!(validator.next_epoch_protocol_pubkey_bytes().is_none());
    assert!(validator.next_epoch_proof_of_possession().is_none());
    assert!(validator.next_epoch_worker_pubkey_bytes().is_none());
    assert!(validator.next_epoch_network_pubkey_bytes().is_none());
}
