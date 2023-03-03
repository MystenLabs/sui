// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This file contains tests testing functionalities in `sui_system` that are not
// already tested by the other more themed tests such as `delegation_tests` or
// `rewards_distribution_tests`.

#[test_only]
module sui::sui_system_tests {
    use sui::test_scenario::{Self, Scenario};
    use sui::sui::SUI;
    use sui::governance_test_utils::{add_validator, advance_epoch, remove_validator, set_up_sui_system_state, create_sui_system_state_for_testing};
    use sui::sui_system::{Self, SuiSystemState};
    use sui::validator::Self;
    use sui::vec_set;
    use sui::table;
    use std::vector;
    use sui::coin;
    use sui::balance;
    use sui::validator::Validator;
    use sui::test_utils::assert_eq;
    use std::option::Self;
    use sui::url;
    use std::string;
    use std::ascii;

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

    fun update_metadata(
        scenario: &mut Scenario,
        system_state: &mut SuiSystemState,
        name: vector<u8>,
        protocol_pub_key: vector<u8>,
        pop: vector<u8>,
        network_address: vector<u8>,
        p2p_address: vector<u8>
    ) {
        let ctx = test_scenario::ctx(scenario);
        sui_system::update_validator_name(system_state, name, ctx);
        sui_system::update_validator_description(system_state, b"new_desc", ctx);
        sui_system::update_validator_image_url(system_state, b"new_image_url", ctx);
        sui_system::update_validator_project_url(system_state, b"new_project_url", ctx);
        sui_system::update_validator_next_epoch_network_address(system_state, network_address, ctx);
        sui_system::update_validator_next_epoch_p2p_address(system_state, p2p_address, ctx);
        sui_system::update_validator_next_epoch_primary_address(system_state, vector[4, 168, 168, 168, 168], ctx);
        sui_system::update_validator_next_epoch_worker_address(system_state, vector[4, 168, 168, 168, 168], ctx);
        sui_system::update_validator_next_epoch_protocol_pubkey(
            system_state,
            protocol_pub_key,
            pop,
            ctx
        );
        sui_system::update_validator_next_epoch_worker_pubkey(system_state, vector[215, 64, 85, 185, 231, 116, 69, 151, 97, 79, 4, 183, 20, 70, 84, 51, 211, 162, 115, 221, 73, 241, 240, 171, 192, 25, 232, 106, 175, 162, 176, 43], ctx);
        sui_system::update_validator_next_epoch_network_pubkey(system_state, vector[148, 117, 212, 171, 44, 104, 167, 11, 177, 100, 4, 55, 17, 235, 117, 45, 117, 84, 159, 49, 14, 159, 239, 246, 237, 21, 83, 166, 112, 53, 62, 199], ctx);
    }

    fun verify_metadata(
        validator: &Validator,
        name: vector<u8>,
        protocol_pub_key: vector<u8>,
        pop: vector<u8>,
        network_address: vector<u8>,
        p2p_address: vector<u8>,
        new_protocol_pub_key: vector<u8>,
        new_pop: vector<u8>,
        new_network_address: vector<u8>,
        new_p2p_address: vector<u8>,
    ) {
        // Current epoch
        assert!(validator::name(validator) == &string::from_ascii(ascii::string(name)), 0);
        assert!(validator::description(validator) == &string::from_ascii(ascii::string(b"new_desc")), 0);
        assert!(validator::image_url(validator) == &url::new_unsafe_from_bytes(b"new_image_url"), 0);
        assert!(validator::project_url(validator) == &url::new_unsafe_from_bytes(b"new_project_url"), 0);
        assert!(validator::network_address(validator) == &network_address, 0);
        assert!(validator::p2p_address(validator) == &p2p_address, 0);
        assert!(validator::primary_address(validator) == &vector[4, 127, 0, 0, 1], 0);
        assert!(validator::worker_address(validator) == &vector[4, 127, 0, 0, 1], 0);
        assert!(validator::protocol_pubkey_bytes(validator) == &protocol_pub_key, 0);
        assert!(validator::proof_of_possession(validator) == &pop, 0);
        assert!(validator::network_pubkey_bytes(validator) == &vector[32, 219, 38, 23, 242, 109, 116, 235, 225, 192, 219, 45, 40, 124, 162, 25, 33, 68, 52, 41, 123, 9, 98, 11, 184, 150, 214, 62, 60, 210, 121, 62], 0);
        assert!(validator::worker_pubkey_bytes(validator) == &vector[68, 55, 206, 25, 199, 14, 169, 53, 68, 92, 142, 136, 174, 149, 54, 215, 101, 63, 249, 206, 197, 98, 233, 80, 60, 12, 183, 32, 216, 88, 103, 25], 0);

        // Next epoch
        assert!(validator::next_epoch_network_address(validator) == &option::some(new_network_address), 0);
        assert!(validator::next_epoch_p2p_address(validator) == &option::some(new_p2p_address), 0);
        assert!(validator::next_epoch_primary_address(validator) == &option::some(vector[4, 168, 168, 168, 168]), 0);
        assert!(validator::next_epoch_worker_address(validator) == &option::some(vector[4, 168, 168, 168, 168]), 0);
        assert!(
            validator::next_epoch_protocol_pubkey_bytes(validator) == &option::some(new_protocol_pub_key),
            0
        );
        assert!(
            validator::next_epoch_proof_of_possession(validator) == &option::some(new_pop),
            0
        );
        assert!(
            validator::next_epoch_worker_pubkey_bytes(validator) == &option::some(vector[215, 64, 85, 185, 231, 116, 69, 151, 97, 79, 4, 183, 20, 70, 84, 51, 211, 162, 115, 221, 73, 241, 240, 171, 192, 25, 232, 106, 175, 162, 176, 43]),
            0
        );
        assert!(
            validator::next_epoch_network_pubkey_bytes(validator) == &option::some(vector[148, 117, 212, 171, 44, 104, 167, 11, 177, 100, 4, 55, 17, 235, 117, 45, 117, 84, 159, 49, 14, 159, 239, 246, 237, 21, 83, 166, 112, 53, 62, 199]),
            0
        );
    }

    fun verify_metadata_after_advancing_epoch(
        validator: &Validator,
        name: vector<u8>,
        protocol_pub_key: vector<u8>,
        pop: vector<u8>,
        network_address: vector<u8>,
        p2p_address: vector<u8>,
    ) {
        // Current epoch
        assert!(validator::name(validator) == &string::from_ascii(ascii::string(name)), 0);
        assert!(validator::description(validator) == &string::from_ascii(ascii::string(b"new_desc")), 0);
        assert!(validator::image_url(validator) == &url::new_unsafe_from_bytes(b"new_image_url"), 0);
        assert!(validator::project_url(validator) == &url::new_unsafe_from_bytes(b"new_project_url"), 0);
        assert!(validator::network_address(validator) == &network_address, 0);
        assert!(validator::p2p_address(validator) == &p2p_address, 0);
        assert!(validator::primary_address(validator) == &vector[4, 168, 168, 168, 168], 0);
        assert!(validator::worker_address(validator) == &vector[4, 168, 168, 168, 168], 0);
        assert!(validator::protocol_pubkey_bytes(validator) == &protocol_pub_key, 0);
        assert!(validator::proof_of_possession(validator) == &pop, 0);
        assert!(validator::worker_pubkey_bytes(validator) == &vector[215, 64, 85, 185, 231, 116, 69, 151, 97, 79, 4, 183, 20, 70, 84, 51, 211, 162, 115, 221, 73, 241, 240, 171, 192, 25, 232, 106, 175, 162, 176, 43], 0);
        assert!(validator::network_pubkey_bytes(validator) == &vector[148, 117, 212, 171, 44, 104, 167, 11, 177, 100, 4, 55, 17, 235, 117, 45, 117, 84, 159, 49, 14, 159, 239, 246, 237, 21, 83, 166, 112, 53, 62, 199], 0);

        // Next epoch
        assert!(option::is_none(validator::next_epoch_network_address(validator)), 0);
        assert!(option::is_none(validator::next_epoch_p2p_address(validator)), 0);
        assert!(option::is_none(validator::next_epoch_primary_address(validator)), 0);
        assert!(option::is_none(validator::next_epoch_worker_address(validator)), 0);
        assert!(option::is_none(validator::next_epoch_protocol_pubkey_bytes(validator)), 0);
        assert!(option::is_none(validator::next_epoch_proof_of_possession(validator)), 0);
        assert!(option::is_none(validator::next_epoch_worker_pubkey_bytes(validator)), 0);
        assert!(option::is_none(validator::next_epoch_network_pubkey_bytes(validator)), 0);
    }

    #[test]
    fun test_active_validator_update_metadata() {
        let validator_addr = @0xaf76afe6f866d8426d2be85d6ef0b11f871a251d043b2f11e15563bf418f5a5a;
        let scenario_val = test_scenario::begin(validator_addr);
        let scenario = &mut scenario_val;

        // Set up SuiSystemState with an active validator
        let validators = vector::empty();
        let ctx = test_scenario::ctx(scenario);
        let validator = validator::new_for_testing(
            validator_addr,
            vector[153, 242, 94, 246, 31, 128, 50, 185, 20, 99, 100, 96, 152, 44, 92, 198, 241, 52, 239, 29, 218, 231, 102, 87, 242, 203, 254, 193, 235, 252, 141, 9, 115, 116, 8, 13, 246, 252, 240, 220, 184, 188, 75, 13, 142, 10, 245, 216, 14, 187, 255, 43, 76, 89, 159, 84, 244, 45, 99, 18, 223, 195, 20, 39, 96, 120, 193, 204, 52, 126, 187, 190, 197, 25, 139, 226, 88, 81, 63, 56, 107, 147, 13, 2, 194, 116, 154, 128, 62, 35, 48, 149, 94, 189, 26, 16],
            vector[32, 219, 38, 23, 242, 109, 116, 235, 225, 192, 219, 45, 40, 124, 162, 25, 33, 68, 52, 41, 123, 9, 98, 11, 184, 150, 214, 62, 60, 210, 121, 62],
            vector[68, 55, 206, 25, 199, 14, 169, 53, 68, 92, 142, 136, 174, 149, 54, 215, 101, 63, 249, 206, 197, 98, 233, 80, 60, 12, 183, 32, 216, 88, 103, 25],
            vector[170, 123, 102, 14, 115, 218, 115, 118, 170, 89, 192, 247, 101, 58, 60, 31, 48, 30, 9, 47, 0, 59, 54, 9, 136, 148, 14, 159, 198, 205, 109, 33, 189, 144, 195, 122, 18, 111, 137, 207, 112, 77, 204, 241, 187, 152, 88, 238],
            b"ValidatorName",
            b"description",
            b"image_url",
            b"project_url",
            vector[4, 127, 0, 0, 1],
            vector[4, 127, 0, 0, 1],
            vector[4, 127, 0, 0, 1],
            vector[4, 127, 0, 0, 1],
            balance::create_for_testing<SUI>(100),
            option::none(),
            1,
            0,
            0,
            ctx
        );
        vector::push_back(&mut validators, validator);
        create_sui_system_state_for_testing(validators, 1000, 0, ctx);

        test_scenario::next_tx(scenario, validator_addr);

        let system_state = test_scenario::take_shared<SuiSystemState>(scenario);

        // Test active validator metadata changes
        test_scenario::next_tx(scenario, validator_addr); 
        {
            update_metadata(
                scenario,
                &mut system_state,
                b"validator_new_name",
                vector[143, 97, 231, 116, 194, 3, 239, 10, 180, 80, 18, 78, 135, 46, 201, 7, 72, 33, 52, 183, 108, 35, 55, 55, 38, 187, 187, 150, 233, 146, 117, 165, 157, 219, 220, 157, 150, 19, 224, 131, 23, 206, 189, 221, 55, 134, 90, 140, 21, 159, 246, 179, 108, 104, 152, 249, 176, 243, 55, 27, 154, 78, 142, 169, 64, 77, 159, 227, 43, 123, 35, 252, 28, 205, 209, 160, 249, 40, 110, 101, 55, 16, 176, 56, 56, 177, 123, 185, 58, 61, 63, 88, 239, 241, 95, 99],
                vector[161, 130, 28, 216, 188, 134, 52, 4, 25, 167, 187, 251, 207, 203, 145, 37, 30, 135, 202, 189, 170, 87, 115, 250, 82, 59, 216, 9, 150, 110, 52, 167, 225, 17, 132, 192, 32, 41, 20, 124, 115, 54, 158, 228, 55, 75, 98, 36],
                vector[4, 42, 42, 42, 42],
                vector[4, 43, 43, 43, 43],
            );
        };

        test_scenario::next_tx(scenario, validator_addr);
        let validator = sui_system::active_validator_by_address(&system_state, validator_addr);
        verify_metadata(
            validator,
            b"validator_new_name",
            vector[153, 242, 94, 246, 31, 128, 50, 185, 20, 99, 100, 96, 152, 44, 92, 198, 241, 52, 239, 29, 218, 231, 102, 87, 242, 203, 254, 193, 235, 252, 141, 9, 115, 116, 8, 13, 246, 252, 240, 220, 184, 188, 75, 13, 142, 10, 245, 216, 14, 187, 255, 43, 76, 89, 159, 84, 244, 45, 99, 18, 223, 195, 20, 39, 96, 120, 193, 204, 52, 126, 187, 190, 197, 25, 139, 226, 88, 81, 63, 56, 107, 147, 13, 2, 194, 116, 154, 128, 62, 35, 48, 149, 94, 189, 26, 16],
            vector[170, 123, 102, 14, 115, 218, 115, 118, 170, 89, 192, 247, 101, 58, 60, 31, 48, 30, 9, 47, 0, 59, 54, 9, 136, 148, 14, 159, 198, 205, 109, 33, 189, 144, 195, 122, 18, 111, 137, 207, 112, 77, 204, 241, 187, 152, 88, 238],
            vector[4, 127, 0, 0, 1],
            vector[4, 127, 0, 0, 1],
            vector[143, 97, 231, 116, 194, 3, 239, 10, 180, 80, 18, 78, 135, 46, 201, 7, 72, 33, 52, 183, 108, 35, 55, 55, 38, 187, 187, 150, 233, 146, 117, 165, 157, 219, 220, 157, 150, 19, 224, 131, 23, 206, 189, 221, 55, 134, 90, 140, 21, 159, 246, 179, 108, 104, 152, 249, 176, 243, 55, 27, 154, 78, 142, 169, 64, 77, 159, 227, 43, 123, 35, 252, 28, 205, 209, 160, 249, 40, 110, 101, 55, 16, 176, 56, 56, 177, 123, 185, 58, 61, 63, 88, 239, 241, 95, 99],
            vector[161, 130, 28, 216, 188, 134, 52, 4, 25, 167, 187, 251, 207, 203, 145, 37, 30, 135, 202, 189, 170, 87, 115, 250, 82, 59, 216, 9, 150, 110, 52, 167, 225, 17, 132, 192, 32, 41, 20, 124, 115, 54, 158, 228, 55, 75, 98, 36],
            vector[4 ,42, 42, 42, 42],
            vector[4, 43, 43, 43, 43],
        );

        test_scenario::return_shared(system_state);
        test_scenario::end(scenario_val);

        // Test pending validator metadata changes
        let new_validator_addr = @0x8e3446145b0c7768839d71840df389ffa3b9742d0baaff326a3d453b595f87d7;
        let scenario_val = test_scenario::begin(new_validator_addr);
        let scenario = &mut scenario_val;
        let system_state = test_scenario::take_shared<SuiSystemState>(scenario);
        test_scenario::next_tx(scenario, new_validator_addr);
        {
            let ctx = test_scenario::ctx(scenario);
            sui_system::request_add_validator(
                &mut system_state,
                vector[153, 21, 95, 72, 205, 126, 148, 249, 194, 129, 121, 224, 137, 171, 173, 206, 207, 69, 3, 142, 106, 91, 158, 244, 0, 234, 14, 134, 130, 255, 173, 137, 125, 109, 44, 193, 187, 107, 78, 227, 84, 147, 66, 54, 92, 53, 208, 76, 10, 110, 217, 188, 125, 75, 58, 1, 143, 160, 113, 62, 239, 45, 154, 163, 105, 227, 253, 87, 44, 156, 5, 211, 41, 8, 35, 13, 197, 240, 203, 104, 222, 70, 62, 189, 63, 228, 214, 32, 82, 119, 148, 170, 155, 82, 223, 127],
                vector[32, 219, 38, 23, 242, 109, 116, 235, 225, 192, 219, 45, 40, 124, 162, 25, 33, 68, 52, 41, 123, 9, 98, 11, 184, 150, 214, 62, 60, 210, 121, 62],
                vector[68, 55, 206, 25, 199, 14, 169, 53, 68, 92, 142, 136, 174, 149, 54, 215, 101, 63, 249, 206, 197, 98, 233, 80, 60, 12, 183, 32, 216, 88, 103, 25],
                vector[131, 170, 51, 121, 46, 85, 50, 42, 110, 180, 220, 186, 24, 12, 168, 180, 66, 63, 129, 111, 6, 94, 250, 52, 137, 174, 6, 184, 181, 148, 15, 5, 129, 14, 8, 206, 163, 32, 239, 20, 141, 242, 195, 80, 179, 142, 35, 13],
                b"ValidatorName2",
                b"description2",
                b"image_url2",
                b"project_url2",
                vector[4, 127, 0, 0, 2],
                vector[4, 127, 0, 0, 2],
                vector[4, 127, 0, 0, 1],
                vector[4, 127, 0, 0, 1],
                coin::mint_for_testing(100, ctx),
                1,
                0,
                ctx,
            );
        };

        test_scenario::next_tx(scenario, new_validator_addr); 
        {
            update_metadata(
                scenario,
                &mut system_state,
                b"new_validator_new_name",
                vector[183, 97, 159, 105, 112, 198, 200, 131, 106, 12, 121, 10, 13, 215, 126, 169, 100, 14, 36, 38, 62, 247, 25, 195, 136, 153, 95, 72, 35, 138, 154, 215, 92, 221, 78, 232, 48, 200, 86, 101, 12, 48, 67, 190, 41, 198, 188, 88, 20, 209, 164, 224, 162, 239, 20, 222, 216, 229, 31, 200, 168, 65, 198, 231, 26, 26, 128, 29, 83, 103, 124, 130, 202, 100, 167, 34, 172, 124, 60, 74, 223, 77, 61, 171, 226, 24, 81, 221, 56, 157, 217, 170, 63, 153, 56, 166],
                vector[144, 93, 160, 14, 152, 139, 5, 150, 41, 172, 63, 158, 49, 33, 86, 5, 189, 115, 168, 91, 111, 159, 77, 32, 172, 15, 170, 71, 201, 212, 59, 149, 199, 17, 16, 46, 210, 1, 221, 171, 35, 11, 249, 128, 220, 111, 64, 64],
                vector[4, 66, 66, 66, 66],
                vector[4, 77, 77, 77, 77],
            );
        };

        test_scenario::next_tx(scenario, new_validator_addr);
        let validator = sui_system::pending_validator_by_address(&system_state, new_validator_addr);
        verify_metadata(
            validator,
            b"new_validator_new_name",
            vector[153, 21, 95, 72, 205, 126, 148, 249, 194, 129, 121, 224, 137, 171, 173, 206, 207, 69, 3, 142, 106, 91, 158, 244, 0, 234, 14, 134, 130, 255, 173, 137, 125, 109, 44, 193, 187, 107, 78, 227, 84, 147, 66, 54, 92, 53, 208, 76, 10, 110, 217, 188, 125, 75, 58, 1, 143, 160, 113, 62, 239, 45, 154, 163, 105, 227, 253, 87, 44, 156, 5, 211, 41, 8, 35, 13, 197, 240, 203, 104, 222, 70, 62, 189, 63, 228, 214, 32, 82, 119, 148, 170, 155, 82, 223, 127],
            vector[131, 170, 51, 121, 46, 85, 50, 42, 110, 180, 220, 186, 24, 12, 168, 180, 66, 63, 129, 111, 6, 94, 250, 52, 137, 174, 6, 184, 181, 148, 15, 5, 129, 14, 8, 206, 163, 32, 239, 20, 141, 242, 195, 80, 179, 142, 35, 13],
            vector[4, 127, 0, 0, 2],
            vector[4, 127, 0, 0, 2],
            vector[183, 97, 159, 105, 112, 198, 200, 131, 106, 12, 121, 10, 13, 215, 126, 169, 100, 14, 36, 38, 62, 247, 25, 195, 136, 153, 95, 72, 35, 138, 154, 215, 92, 221, 78, 232, 48, 200, 86, 101, 12, 48, 67, 190, 41, 198, 188, 88, 20, 209, 164, 224, 162, 239, 20, 222, 216, 229, 31, 200, 168, 65, 198, 231, 26, 26, 128, 29, 83, 103, 124, 130, 202, 100, 167, 34, 172, 124, 60, 74, 223, 77, 61, 171, 226, 24, 81, 221, 56, 157, 217, 170, 63, 153, 56, 166],
            vector[144, 93, 160, 14, 152, 139, 5, 150, 41, 172, 63, 158, 49, 33, 86, 5, 189, 115, 168, 91, 111, 159, 77, 32, 172, 15, 170, 71, 201, 212, 59, 149, 199, 17, 16, 46, 210, 1, 221, 171, 35, 11, 249, 128, 220, 111, 64, 64],
            vector[4, 66, 66, 66, 66],
            vector[4, 77, 77, 77, 77],
        );

        test_scenario::return_shared(system_state);

        // Advance epoch to effectuate the metadata changes.
        test_scenario::next_tx(scenario, new_validator_addr);
        advance_epoch(scenario);

        // Now both validators are active, verify their metadata.
        test_scenario::next_tx(scenario, new_validator_addr);
        let system_state = test_scenario::take_shared<SuiSystemState>(scenario);
        let validator = sui_system::active_validator_by_address(&system_state, validator_addr);
        verify_metadata_after_advancing_epoch(
            validator,
            b"validator_new_name",
            vector[143, 97, 231, 116, 194, 3, 239, 10, 180, 80, 18, 78, 135, 46, 201, 7, 72, 33, 52, 183, 108, 35, 55, 55, 38, 187, 187, 150, 233, 146, 117, 165, 157, 219, 220, 157, 150, 19, 224, 131, 23, 206, 189, 221, 55, 134, 90, 140, 21, 159, 246, 179, 108, 104, 152, 249, 176, 243, 55, 27, 154, 78, 142, 169, 64, 77, 159, 227, 43, 123, 35, 252, 28, 205, 209, 160, 249, 40, 110, 101, 55, 16, 176, 56, 56, 177, 123, 185, 58, 61, 63, 88, 239, 241, 95, 99],
            vector[161, 130, 28, 216, 188, 134, 52, 4, 25, 167, 187, 251, 207, 203, 145, 37, 30, 135, 202, 189, 170, 87, 115, 250, 82, 59, 216, 9, 150, 110, 52, 167, 225, 17, 132, 192, 32, 41, 20, 124, 115, 54, 158, 228, 55, 75, 98, 36],
            vector[4, 42, 42, 42, 42],
            vector[4, 43, 43, 43, 43],
        );

        let validator = sui_system::active_validator_by_address(&system_state, new_validator_addr);
        verify_metadata_after_advancing_epoch(
            validator,
            b"new_validator_new_name",
            vector[183, 97, 159, 105, 112, 198, 200, 131, 106, 12, 121, 10, 13, 215, 126, 169, 100, 14, 36, 38, 62, 247, 25, 195, 136, 153, 95, 72, 35, 138, 154, 215, 92, 221, 78, 232, 48, 200, 86, 101, 12, 48, 67, 190, 41, 198, 188, 88, 20, 209, 164, 224, 162, 239, 20, 222, 216, 229, 31, 200, 168, 65, 198, 231, 26, 26, 128, 29, 83, 103, 124, 130, 202, 100, 167, 34, 172, 124, 60, 74, 223, 77, 61, 171, 226, 24, 81, 221, 56, 157, 217, 170, 63, 153, 56, 166],
            vector[144, 93, 160, 14, 152, 139, 5, 150, 41, 172, 63, 158, 49, 33, 86, 5, 189, 115, 168, 91, 111, 159, 77, 32, 172, 15, 170, 71, 201, 212, 59, 149, 199, 17, 16, 46, 210, 1, 221, 171, 35, 11, 249, 128, 220, 111, 64, 64],
            vector[4, 66, 66, 66, 66],
            vector[4, 77, 77, 77, 77],
        );

        test_scenario::return_shared(system_state);
        test_scenario::end(scenario_val);
    }
}
