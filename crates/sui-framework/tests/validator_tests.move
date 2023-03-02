// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::validator_tests {
    use sui::sui::SUI;
    use sui::test_scenario;
    use sui::validator;
    use sui::url;
    use std::option;
    use std::ascii;
    use std::string;
    use sui::coin::{Self, Coin};
    use sui::balance;
    use sui::staking_pool::{Self, StakedSui};
    use std::vector;


    const VALID_PUBKEY: vector<u8> = vector[153, 242, 94, 246, 31, 128, 50, 185, 20, 99, 100, 96, 152, 44, 92, 198, 241, 52, 239, 29, 218, 231, 102, 87, 242, 203, 254, 193, 235, 252, 141, 9, 115, 116, 8, 13, 246, 252, 240, 220, 184, 188, 75, 13, 142, 10, 245, 216, 14, 187, 255, 43, 76, 89, 159, 84, 244, 45, 99, 18, 223, 195, 20, 39, 96, 120, 193, 204, 52, 126, 187, 190, 197, 25, 139, 226, 88, 81, 63, 56, 107, 147, 13, 2, 194, 116, 154, 128, 62, 35, 48, 149, 94, 189, 26, 16];

    const VALID_NET_PUBKEY: vector<u8> = vector[171, 2, 39, 3, 139, 105, 166, 171, 153, 151, 102, 197, 151, 186, 140, 116, 114, 90, 213, 225, 20, 167, 60, 69, 203, 12, 180, 198, 9, 217, 117, 38];

    const VALID_WORKER_PUBKEY: vector<u8> = vector[171, 2, 39, 3, 139, 105, 166, 171, 153, 151, 102, 197, 151, 186, 140, 116, 114, 90, 213, 225, 20, 167, 60, 69, 203, 12, 180, 198, 9, 217, 117, 38];

    // Proof of possesion generated from sui/crates/sui-types/src/unit_tests/crypto_tests.rs
    const PROOF_OF_POSESSION: vector<u8> = vector[170, 123, 102, 14, 115, 218, 115, 118, 170, 89, 192, 247, 101, 58, 60, 31, 48, 30, 9, 47, 0, 59, 54, 9, 136, 148, 14, 159, 198, 205, 109, 33, 189, 144, 195, 122, 18, 111, 137, 207, 112, 77, 204, 241, 187, 152, 88, 238];

    /// These  equivalent to /ip4/127.0.0.1
    const VALID_NET_ADDR: vector<u8> = vector[4, 127, 0, 0, 1];
    const VALID_P2P_ADDR: vector<u8> = vector[4, 127, 0, 0, 1];
    const VALID_CONSENSUS_ADDR: vector<u8> = vector[4, 127, 0, 0, 1];
    const VALID_WORKER_ADDR: vector<u8> = vector[4, 127, 0, 0, 1];

    #[test]
    fun test_validator_owner_flow() {
        let sender = @0xaf76afe6f866d8426d2be85d6ef0b11f871a251d043b2f11e15563bf418f5a5a;
        let scenario_val = test_scenario::begin(sender);
        let scenario = &mut scenario_val;
        {
            let ctx = test_scenario::ctx(scenario);

            let init_stake = coin::into_balance(coin::mint_for_testing(10, ctx));
            let validator = validator::new(
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
            );
            assert!(validator::total_stake_amount(&validator) == 10, 0);
            assert!(validator::sui_address(&validator) == sender, 0);

            validator::destroy(validator, ctx);
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

        let validator = validator::new(
            sender,
            VALID_PUBKEY,
            VALID_NET_PUBKEY,
            VALID_WORKER_PUBKEY,
            PROOF_OF_POSESSION,
            b"Validator1",
            b"Validator1",
            b"image_url1",
            b"project_url1",
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
        );

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

        validator::destroy(validator, test_scenario::ctx(scenario));
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
    #[expected_failure(abort_code = validator::EMetadataInvalidConsensusAddr)]
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

}
