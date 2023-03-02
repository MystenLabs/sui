// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::validator_tests {
    use sui::sui::SUI;
    use sui::test_scenario;
    use sui::url;
    use std::string::Self;
    use sui::validator::{Self, Validator};
    use sui::tx_context::{Self, TxContext};
    use sui::balance::Balance;
    use std::option;
    use std::ascii;
    use sui::coin::{Self, Coin};
    use sui::balance;
    use sui::staking_pool::{Self, StakedSui};
    use std::vector;
    use sui::test_utils;


    const VALID_PUBKEY: vector<u8> = vector[153, 242, 94, 246, 31, 128, 50, 185, 20, 99, 100, 96, 152, 44, 92, 198, 241, 52, 239, 29, 218, 231, 102, 87, 242, 203, 254, 193, 235, 252, 141, 9, 115, 116, 8, 13, 246, 252, 240, 220, 184, 188, 75, 13, 142, 10, 245, 216, 14, 187, 255, 43, 76, 89, 159, 84, 244, 45, 99, 18, 223, 195, 20, 39, 96, 120, 193, 204, 52, 126, 187, 190, 197, 25, 139, 226, 88, 81, 63, 56, 107, 147, 13, 2, 194, 116, 154, 128, 62, 35, 48, 149, 94, 189, 26, 16];

    const VALID_NET_PUBKEY: vector<u8> = vector[171, 2, 39, 3, 139, 105, 166, 171, 153, 151, 102, 197, 151, 186, 140, 116, 114, 90, 213, 225, 20, 167, 60, 69, 203, 12, 180, 198, 9, 217, 117, 38];

    const VALID_WORKER_PUBKEY: vector<u8> = vector[171, 2, 39, 3, 139, 105, 166, 171, 153, 151, 102, 197, 151, 186, 140, 116, 114, 90, 213, 225, 20, 167, 60, 69, 203, 12, 180, 198, 9, 217, 117, 38];

    // Proof of possesion generated from sui/crates/sui-types/src/unit_tests/crypto_tests.rs
    const PROOF_OF_POSESSION: vector<u8> = vector[170, 123, 102, 14, 115, 218, 115, 118, 170, 89, 192, 247, 101, 58, 60, 31, 48, 30, 9, 47, 0, 59, 54, 9, 136, 148, 14, 159, 198, 205, 109, 33, 189, 144, 195, 122, 18, 111, 137, 207, 112, 77, 204, 241, 187, 152, 88, 238];

    /// These are equivalent to /ip4/127.0.0.1
    const VALID_NET_ADDR: vector<u8> = vector[4, 127, 0, 0, 1];
    const VALID_P2P_ADDR: vector<u8> = vector[4, 127, 0, 0, 1];
    const VALID_CONSENSUS_ADDR: vector<u8> = vector[4, 127, 0, 0, 1];
    const VALID_WORKER_ADDR: vector<u8> = vector[4, 127, 0, 0, 1];

    #[test_only]
    fun get_test_validator(ctx: &mut TxContext, init_stake: Balance<SUI>): Validator {
        let sender = tx_context::sender(ctx);
        validator::new(
            sender,
            VALID_PUBKEY,
            VALID_NET_PUBKEY,
            VALID_WORKER_PUBKEY,
            PROOF_OF_POSESSION,
            b"Validator1",
            b"Validator1",
            b"Validator1",
            b"Validator1",
            VALID_NET_ADDR,
            VALID_P2P_ADDR,
            VALID_CONSENSUS_ADDR,
            VALID_WORKER_ADDR,
            init_stake,
            option::none(),
            1,
            0,
            0,
            ctx
        )
    }

    #[test]
    fun test_validator_owner_flow() {
        let sender = @0xaf76afe6f866d8426d2be85d6ef0b11f871a251d043b2f11e15563bf418f5a5a;
        let scenario_val = test_scenario::begin(sender);
        let scenario = &mut scenario_val;
        {
            let ctx = test_scenario::ctx(scenario);

            let init_stake = coin::into_balance(coin::mint_for_testing(10, ctx));
            let validator = get_test_validator(ctx, init_stake);
            assert!(validator::total_stake_amount(&validator) == 10, 0);
            assert!(validator::sui_address(&validator) == sender, 0);

            test_utils::destroy(validator);
        };

        // Check that after destroy, the original stake still exists.
         test_scenario::next_tx(scenario, sender);
         {
             let stake = test_scenario::take_from_sender<StakedSui>(scenario);
             assert!(staking_pool::staked_sui_amount(&stake) == 10, 0);
             test_scenario::return_to_sender(scenario, stake);
         };
        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_pending_validator_flow() {
        let sender = @0xaf76afe6f866d8426d2be85d6ef0b11f871a251d043b2f11e15563bf418f5a5a;
        let scenario_val = test_scenario::begin(sender);
        let scenario = &mut scenario_val;
        let ctx = test_scenario::ctx(scenario);
        let init_stake = coin::into_balance(coin::mint_for_testing(10, ctx));

        let validator = get_test_validator(ctx, init_stake);
        test_scenario::next_tx(scenario, sender);
        {
            let ctx = test_scenario::ctx(scenario);
            let new_stake = coin::into_balance(coin::mint_for_testing(30, ctx));
            validator::request_add_delegation(&mut validator, new_stake, option::none(), sender, ctx);

            assert!(validator::total_stake(&validator) == 10, 0);
            assert!(validator::pending_stake_amount(&validator) == 30, 0);
        };

        test_scenario::next_tx(scenario, sender);
        {
            let coin_ids = test_scenario::ids_for_sender<StakedSui>(scenario);
            let stake = test_scenario::take_from_sender_by_id<StakedSui>(scenario, *vector::borrow(&coin_ids, 0));
            let ctx = test_scenario::ctx(scenario);
            validator::request_withdraw_delegation(&mut validator, stake, ctx);

            assert!(validator::total_stake(&validator) == 10, 0);
            assert!(validator::pending_stake_amount(&validator) == 30, 0);
            assert!(validator::pending_stake_withdraw_amount(&validator) == 10, 0);

            validator::deposit_delegation_rewards(&mut validator, balance::zero());

            // Calling `process_pending_delegations_and_withdraws` will withdraw the coin and transfer to sender.
            validator::process_pending_delegations_and_withdraws(&mut validator, ctx);

            assert!(validator::total_stake(&validator) == 30, 0);
            assert!(validator::pending_stake_amount(&validator) == 0, 0);
            assert!(validator::pending_stake_withdraw_amount(&validator) == 0, 0);
        };

        test_scenario::next_tx(scenario, sender);
        {
            let coin_ids = test_scenario::ids_for_sender<Coin<SUI>>(scenario);
            let withdraw = test_scenario::take_from_sender_by_id<Coin<SUI>>(scenario, *vector::borrow(&coin_ids, 0));
            assert!(coin::value(&withdraw) == 10, 0);
            test_scenario::return_to_sender(scenario, withdraw);
        };

        test_utils::destroy(validator);
        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_metadata() {
        let metadata = validator::new_metadata(
            @0x42,
            VALID_PUBKEY,
            VALID_NET_PUBKEY,
            VALID_WORKER_PUBKEY,
            PROOF_OF_POSESSION,
            string::from_ascii(ascii::string(b"Validator1")),
            string::from_ascii(ascii::string(b"Validator1")),
            url::new_unsafe_from_bytes(b"image_url1"),
            url::new_unsafe_from_bytes(b"project_url1"),
            VALID_NET_ADDR,
            VALID_P2P_ADDR,
            VALID_CONSENSUS_ADDR,
            VALID_WORKER_ADDR,
        );

        validator::validate_metadata(&metadata);
    }

    #[test]
    #[expected_failure(abort_code = validator::EMetadataInvalidPubKey)]
    fun test_metadata_invalid_pubkey() {
        let metadata = validator::new_metadata(
            @0x42,
            vector[42],
            VALID_NET_PUBKEY,
            VALID_WORKER_PUBKEY,
            PROOF_OF_POSESSION,
            string::from_ascii(ascii::string(b"Validator1")),
            string::from_ascii(ascii::string(b"Validator1")),
            url::new_unsafe_from_bytes(b"image_url1"),
            url::new_unsafe_from_bytes(b"project_url1"),
            VALID_NET_ADDR,
            VALID_P2P_ADDR,
            VALID_CONSENSUS_ADDR,
            VALID_WORKER_ADDR,
        );

        validator::validate_metadata(&metadata);
    }

    #[test]
    #[expected_failure(abort_code = validator::EMetadataInvalidNetPubkey)]
    fun test_metadata_invalid_net_pubkey() {
        let metadata = validator::new_metadata(
            @0x42,
            VALID_PUBKEY,
            vector[42],
            VALID_WORKER_PUBKEY,
            PROOF_OF_POSESSION,
            string::from_ascii(ascii::string(b"Validator1")),
            string::from_ascii(ascii::string(b"Validator1")),
            url::new_unsafe_from_bytes(b"image_url1"),
            url::new_unsafe_from_bytes(b"project_url1"),
            VALID_NET_ADDR,
            VALID_P2P_ADDR,
            VALID_CONSENSUS_ADDR,
            VALID_WORKER_ADDR,
        );

        validator::validate_metadata(&metadata);
    }

    #[test]
    #[expected_failure(abort_code = validator::EMetadataInvalidWorkerPubKey)]
    fun test_metadata_invalid_worker_pubkey() {
        let metadata = validator::new_metadata(
            @0x42,
            VALID_PUBKEY,
            VALID_NET_PUBKEY,
            vector[42],
            PROOF_OF_POSESSION,
            string::from_ascii(ascii::string(b"Validator1")),
            string::from_ascii(ascii::string(b"Validator1")),
            url::new_unsafe_from_bytes(b"image_url1"),
            url::new_unsafe_from_bytes(b"project_url1"),
            VALID_NET_ADDR,
            VALID_P2P_ADDR,
            VALID_CONSENSUS_ADDR,
            VALID_WORKER_ADDR,
        );

        validator::validate_metadata(&metadata);
    }

    #[test]
    #[expected_failure(abort_code = validator::EMetadataInvalidNetAddr)]
    fun test_metadata_invalid_net_addr() {
        let metadata = validator::new_metadata(
            @0x42,
            VALID_PUBKEY,
            VALID_NET_PUBKEY,
            VALID_WORKER_PUBKEY,
            PROOF_OF_POSESSION,
            string::from_ascii(ascii::string(b"Validator1")),
            string::from_ascii(ascii::string(b"Validator1")),
            url::new_unsafe_from_bytes(b"image_url1"),
            url::new_unsafe_from_bytes(b"project_url1"),
            vector[42],
            VALID_P2P_ADDR,
            VALID_CONSENSUS_ADDR,
            VALID_WORKER_ADDR,
        );

        validator::validate_metadata(&metadata);
    }

    #[test]
    #[expected_failure(abort_code = validator::EMetadataInvalidP2pAddr)]
    fun test_metadata_invalid_p2p_addr() {
        let metadata = validator::new_metadata(
            @0x42,
            VALID_PUBKEY,
            VALID_NET_PUBKEY,
            VALID_WORKER_PUBKEY,
            PROOF_OF_POSESSION,
            string::from_ascii(ascii::string(b"Validator1")),
            string::from_ascii(ascii::string(b"Validator1")),
            url::new_unsafe_from_bytes(b"image_url1"),
            url::new_unsafe_from_bytes(b"project_url1"),
            VALID_NET_ADDR,
            vector[42],
            VALID_P2P_ADDR,
            VALID_WORKER_ADDR,
        );

        validator::validate_metadata(&metadata);
    }

    #[test]
    #[expected_failure(abort_code = validator::EMetadataInvalidPrimaryAddr)]
    fun test_metadata_invalid_consensus_addr() {
        let metadata = validator::new_metadata(
            @0x42,
            VALID_PUBKEY,
            VALID_NET_PUBKEY,
            VALID_WORKER_PUBKEY,
            PROOF_OF_POSESSION,
            string::from_ascii(ascii::string(b"Validator1")),
            string::from_ascii(ascii::string(b"Validator1")),
            url::new_unsafe_from_bytes(b"image_url1"),
            url::new_unsafe_from_bytes(b"project_url1"),
            VALID_NET_ADDR,
            VALID_P2P_ADDR,
            vector[42],
            VALID_WORKER_ADDR,
        );

        validator::validate_metadata(&metadata);
    }

    #[test]
    #[expected_failure(abort_code = validator::EMetadataInvalidWorkerAddr)]
    fun test_metadata_invalid_worker_addr() {
        let metadata = validator::new_metadata(
            @0x42,
            VALID_PUBKEY,
            VALID_NET_PUBKEY,
            VALID_WORKER_PUBKEY,
            PROOF_OF_POSESSION,
            string::from_ascii(ascii::string(b"Validator1")),
            string::from_ascii(ascii::string(b"Validator1")),
            url::new_unsafe_from_bytes(b"image_url1"),
            url::new_unsafe_from_bytes(b"project_url1"),
            VALID_NET_ADDR,
            VALID_P2P_ADDR,
            VALID_CONSENSUS_ADDR,
            vector[42],
        );

        validator::validate_metadata(&metadata);
    }

    #[test]
    fun test_validator_update_metadata_ok() {
        let sender = @0xaf76afe6f866d8426d2be85d6ef0b11f871a251d043b2f11e15563bf418f5a5a;
        let scenario_val = test_scenario::begin(sender);
        let scenario = &mut scenario_val;
        let ctx = test_scenario::ctx(scenario);
        let init_stake = coin::into_balance(coin::mint_for_testing(10, ctx));
        let new_protocol_pub_key = vector[143, 97, 231, 116, 194, 3, 239, 10, 180, 80, 18, 78, 135, 46, 201, 7, 72, 33, 52, 183, 108, 35, 55, 55, 38, 187, 187, 150, 233, 146, 117, 165, 157, 219, 220, 157, 150, 19, 224, 131, 23, 206, 189, 221, 55, 134, 90, 140, 21, 159, 246, 179, 108, 104, 152, 249, 176, 243, 55, 27, 154, 78, 142, 169, 64, 77, 159, 227, 43, 123, 35, 252, 28, 205, 209, 160, 249, 40, 110, 101, 55, 16, 176, 56, 56, 177, 123, 185, 58, 61, 63, 88, 239, 241, 95, 99];
        let new_pop = vector[161, 130, 28, 216, 188, 134, 52, 4, 25, 167, 187, 251, 207, 203, 145, 37, 30, 135, 202, 189, 170, 87, 115, 250, 82, 59, 216, 9, 150, 110, 52, 167, 225, 17, 132, 192, 32, 41, 20, 124, 115, 54, 158, 228, 55, 75, 98, 36];
        let new_worker_pub_key = vector[115, 220, 238, 151, 134, 159, 173, 41, 80, 2, 66, 196, 61, 17, 191, 76, 103, 39, 246, 127, 171, 85, 19, 235, 210, 106, 97, 97, 116, 48, 244, 191];
        let new_network_pub_key = vector[149, 128, 161, 13, 11, 183, 96, 45, 89, 20, 188, 205, 26, 127, 147, 254, 184, 229, 184, 102, 64, 170, 104, 29, 191, 171, 91, 99, 58, 178, 41, 156];

        let validator = get_test_validator(ctx, init_stake);

        test_scenario::next_tx(scenario, sender);
        {
            validator::update_next_epoch_network_address(&mut validator, vector[4, 192, 168, 1, 1]);
            validator::update_next_epoch_p2p_address(&mut validator, vector[4, 192, 168, 1, 1]);
            validator::update_next_epoch_primary_address(&mut validator, vector[4, 192, 168, 1, 1]);
            validator::update_next_epoch_worker_address(&mut validator, vector[4, 192, 168, 1, 1]);
            validator::update_next_epoch_protocol_pubkey(
                &mut validator,
                new_protocol_pub_key,
                new_pop,
            );
            validator::update_next_epoch_worker_pubkey(
                &mut validator,
                new_worker_pub_key,
            );
            validator::update_next_epoch_network_pubkey(
                &mut validator,
                new_network_pub_key,
            );

            validator::update_name(&mut validator, string::from_ascii(ascii::string(b"new_name")));
            validator::update_description(&mut validator, string::from_ascii(ascii::string(b"new_desc")));
            validator::update_image_url(&mut validator, url::new_unsafe_from_bytes(b"new_image_url"));
            validator::update_project_url(&mut validator, url::new_unsafe_from_bytes(b"new_proj_url"));
        };

        test_scenario::next_tx(scenario, sender);
        {
            // Current epoch
            assert!(validator::name(&mut validator) == &string::from_ascii(ascii::string(b"new_name")), 0);
            assert!(validator::description(&mut validator) == &string::from_ascii(ascii::string(b"new_desc")), 0);
            assert!(validator::image_url(&mut validator) == &url::new_unsafe_from_bytes(b"new_image_url"), 0);
            assert!(validator::project_url(&mut validator) == &url::new_unsafe_from_bytes(b"new_proj_url"), 0);
            assert!(validator::network_address(&validator) == &VALID_NET_ADDR, 0);
            assert!(validator::p2p_address(&validator) == &VALID_P2P_ADDR, 0);
            assert!(validator::primary_address(&validator) == &VALID_CONSENSUS_ADDR, 0);
            assert!(validator::worker_address(&validator) == &VALID_WORKER_ADDR, 0);
            assert!(validator::protocol_pubkey_bytes(&validator) == &VALID_PUBKEY, 0);
            assert!(validator::proof_of_possession(&validator) == &PROOF_OF_POSESSION, 0);
            assert!(validator::network_pubkey_bytes(&validator) == &VALID_NET_PUBKEY, 0);
            assert!(validator::worker_pubkey_bytes(&validator) == &VALID_WORKER_PUBKEY, 0);

            // Next epoch
            assert!(validator::next_epoch_network_address(&validator) == &option::some(vector[4, 192, 168, 1, 1]), 0);
            assert!(validator::next_epoch_p2p_address(&validator) == &option::some(vector[4, 192, 168, 1, 1]), 0);
            assert!(validator::next_epoch_primary_address(&validator) == &option::some(vector[4, 192, 168, 1, 1]), 0);
            assert!(validator::next_epoch_worker_address(&validator) == &option::some(vector[4, 192, 168, 1, 1]), 0);
            assert!(
                validator::next_epoch_protocol_pubkey_bytes(&validator) == &option::some(new_protocol_pub_key),
                0
            );
            assert!(
                validator::next_epoch_proof_of_possession(&validator) == &option::some(new_pop),
                0
            );
            assert!(
                validator::next_epoch_worker_pubkey_bytes(&validator) == &option::some(new_worker_pub_key),
                0
            );
            assert!(
                validator::next_epoch_network_pubkey_bytes(&validator) == &option::some(new_network_pub_key),
                0
            );
        };

        test_utils::destroy(validator);
        test_scenario::end(scenario_val);
    }

    #[expected_failure(abort_code = sui::validator::EInvalidProofOfPossession)]
    #[test]
    fun test_validator_update_metadata_invalid_proof_of_possession() {
        let sender = @0xaf76afe6f866d8426d2be85d6ef0b11f871a251d043b2f11e15563bf418f5a5a;
        let scenario_val = test_scenario::begin(sender);
        let scenario = &mut scenario_val;
        let ctx = test_scenario::ctx(scenario);
        let init_stake = coin::into_balance(coin::mint_for_testing(10, ctx));

        let validator = get_test_validator(ctx, init_stake);

        test_scenario::next_tx(scenario, sender);
        {
            validator::update_next_epoch_protocol_pubkey(
                &mut validator,
                vector[143, 97, 231, 116, 194, 3, 239, 10, 180, 80, 18, 78, 135, 46, 201, 7, 72, 33, 52, 183, 108, 35, 55, 55, 38, 187, 187, 150, 233, 146, 117, 165, 157, 219, 220, 157, 150, 19, 224, 131, 23, 206, 189, 221, 55, 134, 90, 140, 21, 159, 246, 179, 108, 104, 152, 249, 176, 243, 55, 27, 154, 78, 142, 169, 64, 77, 159, 227, 43, 123, 35, 252, 28, 205, 209, 160, 249, 40, 110, 101, 55, 16, 176, 56, 56, 177, 123, 185, 58, 61, 63, 88, 239, 241, 95, 99],
                // This is an invalid proof of possession, so we abort
                vector[111, 130, 28, 216, 188, 134, 52, 4, 25, 167, 187, 251, 207, 203, 145, 37, 30, 135, 202, 189, 170, 87, 115, 250, 82, 59, 216, 9, 150, 110, 52, 167, 225, 17, 132, 192, 32, 41, 20, 124, 115, 54, 158, 228, 55, 75, 98, 36],
            );
        };

        test_utils::destroy(validator);
        test_scenario::end(scenario_val);
    }

    #[expected_failure(abort_code = sui::validator::EMetadataInvalidNetPubkey)]
    #[test]
    fun test_validator_update_metadata_invalid_network_key() {
        let sender = @0xaf76afe6f866d8426d2be85d6ef0b11f871a251d043b2f11e15563bf418f5a5a;
        let scenario_val = test_scenario::begin(sender);
        let scenario = &mut scenario_val;
        let ctx = test_scenario::ctx(scenario);
        let init_stake = coin::into_balance(coin::mint_for_testing(10, ctx));

        let validator = get_test_validator(ctx, init_stake);

        test_scenario::next_tx(scenario, sender);
        {
            validator::update_next_epoch_network_pubkey(
                &mut validator,
                x"beef",
            );
        };

        test_utils::destroy(validator);
        test_scenario::end(scenario_val);
    }


    #[expected_failure(abort_code = sui::validator::EMetadataInvalidWorkerPubKey)]
    #[test]
    fun test_validator_update_metadata_invalid_worker_key() {
        let sender = @0xaf76afe6f866d8426d2be85d6ef0b11f871a251d043b2f11e15563bf418f5a5a;
        let scenario_val = test_scenario::begin(sender);
        let scenario = &mut scenario_val;
        let ctx = test_scenario::ctx(scenario);
        let init_stake = coin::into_balance(coin::mint_for_testing(10, ctx));

        let validator = get_test_validator(ctx, init_stake);

        test_scenario::next_tx(scenario, sender);
        {
            validator::update_next_epoch_worker_pubkey(
                &mut validator,
                x"beef",
            );
        };

        test_utils::destroy(validator);
        test_scenario::end(scenario_val);
    }

    #[expected_failure(abort_code = sui::validator::EMetadataInvalidNetAddr)]
    #[test]
    fun test_validator_update_metadata_invalid_network_addr() {
        let sender = @0xaf76afe6f866d8426d2be85d6ef0b11f871a251d043b2f11e15563bf418f5a5a;
        let scenario_val = test_scenario::begin(sender);
        let scenario = &mut scenario_val;
        let ctx = test_scenario::ctx(scenario);
        let init_stake = coin::into_balance(coin::mint_for_testing(10, ctx));

        let validator = get_test_validator(ctx, init_stake);

        test_scenario::next_tx(scenario, sender);
        {
            validator::update_next_epoch_network_address(
                &mut validator,
                x"beef",
            );
        };

        test_utils::destroy(validator);
        test_scenario::end(scenario_val);
    }

    #[expected_failure(abort_code = sui::validator::EMetadataInvalidPrimaryAddr)]
    #[test]
    fun test_validator_update_metadata_invalid_consensus_addr() {
        let sender = @0xaf76afe6f866d8426d2be85d6ef0b11f871a251d043b2f11e15563bf418f5a5a;
        let scenario_val = test_scenario::begin(sender);
        let scenario = &mut scenario_val;
        let ctx = test_scenario::ctx(scenario);
        let init_stake = coin::into_balance(coin::mint_for_testing(10, ctx));

        let validator = get_test_validator(ctx, init_stake);

        test_scenario::next_tx(scenario, sender);
        {
            validator::update_next_epoch_primary_address(
                &mut validator,
                x"beef",
            );
        };

        test_utils::destroy(validator);
        test_scenario::end(scenario_val);
    }

    #[expected_failure(abort_code = sui::validator::EMetadataInvalidWorkerAddr)]
    #[test]
    fun test_validator_update_metadata_invalid_worker_addr() {
        let sender = @0xaf76afe6f866d8426d2be85d6ef0b11f871a251d043b2f11e15563bf418f5a5a;
        let scenario_val = test_scenario::begin(sender);
        let scenario = &mut scenario_val;
        let ctx = test_scenario::ctx(scenario);
        let init_stake = coin::into_balance(coin::mint_for_testing(10, ctx));

        let validator = get_test_validator(ctx, init_stake);

        test_scenario::next_tx(scenario, sender);
        {
            validator::update_next_epoch_worker_address(
                &mut validator,
                x"beef",
            );
        };

        test_utils::destroy(validator);
        test_scenario::end(scenario_val);
    }

    #[expected_failure(abort_code = sui::validator::EMetadataInvalidP2pAddr)]
    #[test]
    fun test_validator_update_metadata_invalid_p2p_address() {
        let sender = @0xaf76afe6f866d8426d2be85d6ef0b11f871a251d043b2f11e15563bf418f5a5a;
        let scenario_val = test_scenario::begin(sender);
        let scenario = &mut scenario_val;
        let ctx = test_scenario::ctx(scenario);
        let init_stake = coin::into_balance(coin::mint_for_testing(10, ctx));

        let validator = get_test_validator(ctx, init_stake);

        test_scenario::next_tx(scenario, sender);
        {
            validator::update_next_epoch_p2p_address(
                &mut validator,
                x"beef",
            );
        };

        test_utils::destroy(validator);
        test_scenario::end(scenario_val);
    }
}
