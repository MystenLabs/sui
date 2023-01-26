// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This file contains tests testing functionalities in `sui_system` that are not
// already tested by the other more themed tests such as `delegation_tests` or
// `rewards_distribution_tests`.

#[test_only]
module sui::sui_system_tests {
    use sui::test_scenario::{Self, Scenario};
    use sui::governance_test_utils::{advance_epoch, set_up_sui_system_state};
    use sui::sui_system::{Self, SuiSystemState};
    use sui::vec_set;
    use std::vector;

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
    #[expected_failure(abort_code = sui_system::ENOT_VALIDATOR)]
    fun test_report_non_validator_failure() {
        let scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;

        set_up_sui_system_state(vector[@0x1, @0x2, @0x3], scenario);
        report_helper(@0x1, @0x42, false, scenario);
        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = sui_system::ECANNOT_REPORT_ONESELF)]
    fun test_report_self_failure() {
        let scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;

        set_up_sui_system_state(vector[@0x1, @0x2, @0x3], scenario);
        report_helper(@0x1, @0x1, false, scenario);
        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = sui_system::EREPORT_RECORD_NOT_FOUND)]
    fun test_undo_report_failure() {
        let scenario_val = test_scenario::begin(@0x0);
        let scenario = &mut scenario_val;

        set_up_sui_system_state(vector[@0x1, @0x2, @0x3], scenario);
        report_helper(@0x2, @0x1, true, scenario);
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

    #[test]
    fun test_set_pending_delegation_num() {
        let scenario_val = test_scenario::begin(@0x42);
        let scenario = &mut scenario_val;

        set_up_sui_system_state(vector[@0x1, @0x2, @0x3, @0x4], scenario);

        test_scenario::next_tx(scenario, @0x42);

        let system_state = test_scenario::take_shared<SuiSystemState>(scenario);

        let ctx = test_scenario::ctx(scenario);

        sui_system::set_pending_delegations_num(&mut system_state, 10000, ctx);
        test_scenario::return_shared(system_state);

        advance_epoch(scenario);

        let txn_effects = test_scenario::next_tx(scenario, @0x42);

        std::debug::print(&vector::length(&test_scenario::created(&txn_effects)));

        assert!(vector::length(&test_scenario::created(&txn_effects)) == 10004, 0);

        // After an epoch ends, report records are reset.
        assert!(get_reporters_of(@0x2, scenario) == vector[], 0);

        test_scenario::end(scenario_val);
    }

}
