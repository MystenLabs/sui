// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This file contains tests testing functionalities in `sui_system` that are not
// already tested by the other more themed tests such as `delegation_tests` or
// `rewards_distribution_tests`.

#[test_only]
module sui::sui_system_tests {
    use sui::test_scenario::{Self, Scenario};
    use sui::governance_test_utils::{add_validator, advance_epoch, remove_validator, set_up_sui_system_state};
    use sui::sui_system::{Self, SuiSystemState};
    use sui::vec_set;
    use sui::table;
    use sui::test_utils::assert_eq;

    #[test]
    fun test_report_validator() {
        let scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;

        set_up_sui_system_state(vector[@0x1, @0x2, @0x3], scenario);

        report_helper(@0x1, @0x2, false, scenario);
        assert!(get_reporters_of(@0x2, scenario) == vector[@0x1], 0);
        report_helper(@0x3, @0x2, false, scenario);
        assert!(get_reporters_of(@0x2, scenario) == vector[@0x1, @0x3], 0);

        // Report again and result should stay the same.
        report_helper(@0x1, @0x2, false, scenario);
        assert!(get_reporters_of(@0x2, scenario) == vector[@0x1, @0x3], 0);

        // Undo the report.
        report_helper(@0x3, @0x2, true, scenario);
        assert!(get_reporters_of(@0x2, scenario) == vector[@0x1], 0);

        advance_epoch(scenario);

        // After an epoch ends, report records are reset.
        assert!(get_reporters_of(@0x2, scenario) == vector[], 0);

        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = sui_system::ENotValidator)]
    fun test_report_non_validator_failure() {
        let scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;

        set_up_sui_system_state(vector[@0x1, @0x2, @0x3], scenario);
        report_helper(@0x1, @0x42, false, scenario);
        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = sui_system::ECannotReportOneself)]
    fun test_report_self_failure() {
        let scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;

        set_up_sui_system_state(vector[@0x1, @0x2, @0x3], scenario);
        report_helper(@0x1, @0x1, false, scenario);
        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = sui_system::EReportRecordNotFound)]
    fun test_undo_report_failure() {
        let scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;

        set_up_sui_system_state(vector[@0x1, @0x2, @0x3], scenario);
        report_helper(@0x2, @0x1, true, scenario);
        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_staking_pool_mappings() {
        let scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;

        set_up_sui_system_state(vector[@0x1, @0x2, @0x3], scenario);
        test_scenario::next_tx(scenario, @0x1);
        let system_state = test_scenario::take_shared<SuiSystemState>(scenario);
        let pool_id_1 = sui_system::validator_staking_pool_id(&system_state, @0x1);
        let pool_id_2 = sui_system::validator_staking_pool_id(&system_state, @0x2);
        let pool_id_3 = sui_system::validator_staking_pool_id(&system_state, @0x3);
        let pool_mappings = sui_system::validator_staking_pool_mappings(&system_state);
        assert_eq(table::length(pool_mappings), 3);
        assert_eq(*table::borrow(pool_mappings, pool_id_1), @0x1);
        assert_eq(*table::borrow(pool_mappings, pool_id_2), @0x2);
        assert_eq(*table::borrow(pool_mappings, pool_id_3), @0x3);
        test_scenario::return_shared(system_state);

        let new_validator_addr = @0x1a4623343cd42be47d67314fce0ad042f3c82685544bc91d8c11d24e74ba7357;
        test_scenario::next_tx(scenario, new_validator_addr);
        // This is generated using https://github.com/MystenLabs/sui/blob/375dfb8c56bb422aca8f1592da09a246999bdf4c/crates/sui-types/src/unit_tests/crypto_tests.rs#L38
        let pop = x"8080980b89554e7f03b625ba4104d05d19b523a737e2d09a69d4498a1bcac154fcb29f6334b7e8b99b8f3aa95153232d";
        
        // Add a validator
        add_validator(new_validator_addr, 100, pop, scenario);
        advance_epoch(scenario);

        test_scenario::next_tx(scenario, @0x1);
        let system_state = test_scenario::take_shared<SuiSystemState>(scenario);
        let pool_id_4 = sui_system::validator_staking_pool_id(&system_state, new_validator_addr);
        pool_mappings = sui_system::validator_staking_pool_mappings(&system_state);
        // Check that the previous mappings didn't change as well.
        assert_eq(table::length(pool_mappings), 4);
        assert_eq(*table::borrow(pool_mappings, pool_id_1), @0x1);
        assert_eq(*table::borrow(pool_mappings, pool_id_2), @0x2);
        assert_eq(*table::borrow(pool_mappings, pool_id_3), @0x3);
        assert_eq(*table::borrow(pool_mappings, pool_id_4), new_validator_addr);
        test_scenario::return_shared(system_state);

        // Remove one of the original validators.
        remove_validator(@0x1, scenario);
        advance_epoch(scenario);

        test_scenario::next_tx(scenario, @0x1);
        let system_state = test_scenario::take_shared<SuiSystemState>(scenario);
        pool_mappings = sui_system::validator_staking_pool_mappings(&system_state);
        // Check that the previous mappings didn't change as well.
        assert_eq(table::length(pool_mappings), 3);
        assert_eq(table::contains(pool_mappings, pool_id_1), false);
        assert_eq(*table::borrow(pool_mappings, pool_id_2), @0x2);
        assert_eq(*table::borrow(pool_mappings, pool_id_3), @0x3);
        assert_eq(*table::borrow(pool_mappings, pool_id_4), new_validator_addr);
        test_scenario::return_shared(system_state);

        test_scenario::end(scenario_val);
    }

    fun report_helper(reporter: address, reported: address, is_undo: bool, scenario: &mut Scenario) {
        test_scenario::next_tx(scenario, reporter);

        let system_state = test_scenario::take_shared<SuiSystemState>(scenario);
        let ctx = test_scenario::ctx(scenario);
        if (is_undo) {
            sui_system::undo_report_validator(&mut system_state, reported, ctx);
        } else {
            sui_system::report_validator(&mut system_state, reported, ctx);
        };
        test_scenario::return_shared(system_state);
    }

    fun get_reporters_of(addr: address, scenario: &mut Scenario): vector<address> {
        test_scenario::next_tx(scenario, addr);
        let system_state = test_scenario::take_shared<SuiSystemState>(scenario);
        let res = vec_set::into_keys(sui_system::get_reporters_of(&system_state, addr));
        test_scenario::return_shared(system_state);
        res
    }

}
