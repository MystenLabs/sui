// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui_system::validator_tests {
    use sui::sui::SUI;
    use sui::test_scenario;
    use sui::url;
    use std::string::Self;
    use sui_system::validator::{Self, Validator};
    use sui::tx_context::TxContext;
    use std::option;
    use std::ascii;
    use sui::coin::{Self, Coin};
    use sui::balance;
    use sui_system::staking_pool::{Self, StakedSui};
    use std::vector;
    use sui::test_utils;
    use sui::bag;

    const VALID_NET_PUBKEY: vector<u8> = vector[171, 2, 39, 3, 139, 105, 166, 171, 153, 151, 102, 197, 151, 186, 140, 116, 114, 90, 213, 225, 20, 167, 60, 69, 203, 12, 180, 198, 9, 217, 117, 38];

    const VALID_WORKER_PUBKEY: vector<u8> = vector[171, 3, 39, 3, 139, 105, 166, 171, 153, 151, 102, 197, 151, 186, 140, 116, 114, 90, 213, 225, 20, 167, 60, 69, 203, 12, 180, 198, 9, 217, 117, 38];

    // A valid proof of possession must be generated using the same account address and protocol public key.
    // If either VALID_ADDRESS or VALID_PUBKEY changed, PoP must be regenerated using [fn test_proof_of_possession].
    const VALID_ADDRESS: address = @0xaf76afe6f866d8426d2be85d6ef0b11f871a251d043b2f11e15563bf418f5a5a;
    const VALID_PUBKEY: vector<u8> = x"99f25ef61f8032b914636460982c5cc6f134ef1ddae76657f2cbfec1ebfc8d097374080df6fcf0dcb8bc4b0d8e0af5d80ebbff2b4c599f54f42d6312dfc314276078c1cc347ebbbec5198be258513f386b930d02c2749a803e2330955ebd1a10";
    const PROOF_OF_POSSESSION: vector<u8> = x"b01cc86f421beca7ab4cfca87c0799c4d038c199dd399fbec1924d4d4367866dba9e84d514710b91feb65316e4ceef43";

    const VALID_NET_ADDR: vector<u8> = b"/ip4/127.0.0.1/tcp/80";
    const VALID_P2P_ADDR: vector<u8> = b"/ip4/127.0.0.1/udp/80";
    const VALID_CONSENSUS_ADDR: vector<u8> = b"/ip4/127.0.0.1/udp/80";
    const VALID_WORKER_ADDR: vector<u8> = b"/ip4/127.0.0.1/udp/80";

    const TOO_LONG_257_BYTES: vector<u8> = b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    #[test_only]
    fun get_test_validator(ctx: &mut TxContext): Validator {
        let init_stake = coin::into_balance(coin::mint_for_testing(10_000_000_000, ctx));
        let validator = validator::new(
            VALID_ADDRESS,
            VALID_PUBKEY,
            VALID_NET_PUBKEY,
            VALID_WORKER_PUBKEY,
            PROOF_OF_POSSESSION,
            b"Validator1",
            b"Validator1",
            b"Validator1",
            b"Validator1",
            VALID_NET_ADDR,
            VALID_P2P_ADDR,
            VALID_CONSENSUS_ADDR,
            VALID_WORKER_ADDR,
            1,
            0,
            ctx
        );

        validator::request_add_stake_at_genesis(
            &mut validator,
            init_stake,
            VALID_ADDRESS,
            ctx
        );

        validator::activate(&mut validator, 0);

        validator
    }

    #[test]
    fun test_validator_owner_flow() {
        let sender = VALID_ADDRESS;
        let scenario_val = test_scenario::begin(sender);
        let scenario = &mut scenario_val;
        {
            let ctx = test_scenario::ctx(scenario);

            let validator = get_test_validator(ctx);
            assert!(validator::total_stake_amount(&validator) == 10_000_000_000, 0);
            assert!(validator::sui_address(&validator) == sender, 0);

            test_utils::destroy(validator);
        };

        // Check that after destroy, the original stake still exists.
         test_scenario::next_tx(scenario, sender);
         {
             let stake = test_scenario::take_from_sender<StakedSui>(scenario);
             assert!(staking_pool::staked_sui_amount(&stake) == 10_000_000_000, 0);
             test_scenario::return_to_sender(scenario, stake);
         };
        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_pending_validator_flow() {
        let sender = VALID_ADDRESS;
        let scenario_val = test_scenario::begin(sender);
        let scenario = &mut scenario_val;
        let ctx = test_scenario::ctx(scenario);

        let validator = get_test_validator(ctx);
        test_scenario::next_tx(scenario, sender);
        {
            let ctx = test_scenario::ctx(scenario);
            let new_stake = coin::into_balance(coin::mint_for_testing(30_000_000_000, ctx));
            validator::request_add_stake(&mut validator, new_stake, sender, ctx);

            assert!(validator::total_stake(&validator) == 10_000_000_000, 0);
            assert!(validator::pending_stake_amount(&validator) == 30_000_000_000, 0);
        };

        test_scenario::next_tx(scenario, sender);
        {
            let coin_ids = test_scenario::ids_for_sender<StakedSui>(scenario);
            let stake = test_scenario::take_from_sender_by_id<StakedSui>(scenario, *vector::borrow(&coin_ids, 0));
            let ctx = test_scenario::ctx(scenario);
            validator::request_withdraw_stake(&mut validator, stake, ctx);

            assert!(validator::total_stake(&validator) == 10_000_000_000, 0);
            assert!(validator::pending_stake_amount(&validator) == 30_000_000_000, 0);
            assert!(validator::pending_stake_withdraw_amount(&validator) == 10_000_000_000, 0);

            validator::deposit_stake_rewards(&mut validator, balance::zero());

            // Calling `process_pending_stakes_and_withdraws` will withdraw the coin and transfer to sender.
            validator::process_pending_stakes_and_withdraws(&mut validator, ctx);

            assert!(validator::total_stake(&validator) == 30_000_000_000, 0);
            assert!(validator::pending_stake_amount(&validator) == 0, 0);
            assert!(validator::pending_stake_withdraw_amount(&validator) == 0, 0);
        };

        test_scenario::next_tx(scenario, sender);
        {
            let coin_ids = test_scenario::ids_for_sender<Coin<SUI>>(scenario);
            let withdraw = test_scenario::take_from_sender_by_id<Coin<SUI>>(scenario, *vector::borrow(&coin_ids, 0));
            assert!(coin::value(&withdraw) == 10_000_000_000, 0);
            test_scenario::return_to_sender(scenario, withdraw);
        };

        test_utils::destroy(validator);
        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_metadata() {
        let scenario_val = test_scenario::begin(VALID_ADDRESS);
        let ctx = test_scenario::ctx(&mut scenario_val);
        let metadata = validator::new_metadata(
            VALID_ADDRESS,
            VALID_PUBKEY,
            VALID_NET_PUBKEY,
            VALID_WORKER_PUBKEY,
            PROOF_OF_POSSESSION,
            string::from_ascii(ascii::string(b"Validator1")),
            string::from_ascii(ascii::string(b"Validator1")),
            url::new_unsafe_from_bytes(b"image_url1"),
            url::new_unsafe_from_bytes(b"project_url1"),
            string::from_ascii(ascii::string(VALID_NET_ADDR)),
            string::from_ascii(ascii::string(VALID_P2P_ADDR)),
            string::from_ascii(ascii::string(VALID_CONSENSUS_ADDR)),
            string::from_ascii(ascii::string(VALID_WORKER_ADDR)),
            bag::new(ctx),
        );

        validator::validate_metadata(&metadata);
        test_utils::destroy(metadata);
        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = validator::EMetadataInvalidPubkey)]
    fun test_metadata_invalid_pubkey() {
        let scenario_val = test_scenario::begin(VALID_ADDRESS);
        let ctx = test_scenario::ctx(&mut scenario_val);
        let metadata = validator::new_metadata(
            VALID_ADDRESS,
            vector[42],
            VALID_NET_PUBKEY,
            VALID_WORKER_PUBKEY,
            PROOF_OF_POSSESSION,
            string::from_ascii(ascii::string(b"Validator1")),
            string::from_ascii(ascii::string(b"Validator1")),
            url::new_unsafe_from_bytes(b"image_url1"),
            url::new_unsafe_from_bytes(b"project_url1"),
            string::from_ascii(ascii::string(VALID_NET_ADDR)),
            string::from_ascii(ascii::string(VALID_P2P_ADDR)),
            string::from_ascii(ascii::string(VALID_CONSENSUS_ADDR)),
            string::from_ascii(ascii::string(VALID_WORKER_ADDR)),
            bag::new(ctx),
        );

        validator::validate_metadata(&metadata);
        test_utils::destroy(metadata);
        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = validator::EMetadataInvalidNetPubkey)]
    fun test_metadata_invalid_net_pubkey() {
        let scenario_val = test_scenario::begin(VALID_ADDRESS);
        let ctx = test_scenario::ctx(&mut scenario_val);
        let metadata = validator::new_metadata(
            VALID_ADDRESS,
            VALID_PUBKEY,
            vector[42],
            VALID_WORKER_PUBKEY,
            PROOF_OF_POSSESSION,
            string::from_ascii(ascii::string(b"Validator1")),
            string::from_ascii(ascii::string(b"Validator1")),
            url::new_unsafe_from_bytes(b"image_url1"),
            url::new_unsafe_from_bytes(b"project_url1"),
            string::from_ascii(ascii::string(VALID_NET_ADDR)),
            string::from_ascii(ascii::string(VALID_P2P_ADDR)),
            string::from_ascii(ascii::string(VALID_CONSENSUS_ADDR)),
            string::from_ascii(ascii::string(VALID_WORKER_ADDR)),
            bag::new(ctx),
        );

        validator::validate_metadata(&metadata);
        test_utils::destroy(metadata);
        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = validator::EMetadataInvalidWorkerPubkey)]
    fun test_metadata_invalid_worker_pubkey() {
        let scenario_val = test_scenario::begin(VALID_ADDRESS);
        let ctx = test_scenario::ctx(&mut scenario_val);
        let metadata = validator::new_metadata(
            VALID_ADDRESS,
            VALID_PUBKEY,
            VALID_NET_PUBKEY,
            vector[42],
            PROOF_OF_POSSESSION,
            string::from_ascii(ascii::string(b"Validator1")),
            string::from_ascii(ascii::string(b"Validator1")),
            url::new_unsafe_from_bytes(b"image_url1"),
            url::new_unsafe_from_bytes(b"project_url1"),
            string::from_ascii(ascii::string(VALID_NET_ADDR)),
            string::from_ascii(ascii::string(VALID_P2P_ADDR)),
            string::from_ascii(ascii::string(VALID_CONSENSUS_ADDR)),
            string::from_ascii(ascii::string(VALID_WORKER_ADDR)),
            bag::new(ctx),
        );

        validator::validate_metadata(&metadata);
        test_utils::destroy(metadata);
        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = validator::EMetadataInvalidNetAddr)]
    fun test_metadata_invalid_net_addr() {
        let scenario_val = test_scenario::begin(VALID_ADDRESS);
        let ctx = test_scenario::ctx(&mut scenario_val);
        let metadata = validator::new_metadata(
            VALID_ADDRESS,
            VALID_PUBKEY,
            VALID_NET_PUBKEY,
            VALID_WORKER_PUBKEY,
            PROOF_OF_POSSESSION,
            string::from_ascii(ascii::string(b"Validator1")),
            string::from_ascii(ascii::string(b"Validator1")),
            url::new_unsafe_from_bytes(b"image_url1"),
            url::new_unsafe_from_bytes(b"project_url1"),
            string::from_ascii(ascii::string(b"42")),
            string::from_ascii(ascii::string(VALID_P2P_ADDR)),
            string::from_ascii(ascii::string(VALID_CONSENSUS_ADDR)),
            string::from_ascii(ascii::string(VALID_WORKER_ADDR)),
            bag::new(ctx),
        );

        validator::validate_metadata(&metadata);
        test_utils::destroy(metadata);
        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = validator::EMetadataInvalidP2pAddr)]
    fun test_metadata_invalid_p2p_addr() {
        let scenario_val = test_scenario::begin(VALID_ADDRESS);
        let ctx = test_scenario::ctx(&mut scenario_val);
        let metadata = validator::new_metadata(
            VALID_ADDRESS,
            VALID_PUBKEY,
            VALID_NET_PUBKEY,
            VALID_WORKER_PUBKEY,
            PROOF_OF_POSSESSION,
            string::from_ascii(ascii::string(b"Validator1")),
            string::from_ascii(ascii::string(b"Validator1")),
            url::new_unsafe_from_bytes(b"image_url1"),
            url::new_unsafe_from_bytes(b"project_url1"),
            string::from_ascii(ascii::string(VALID_NET_ADDR)),
            string::from_ascii(ascii::string(b"42")),
            string::from_ascii(ascii::string(VALID_CONSENSUS_ADDR)),
            string::from_ascii(ascii::string(VALID_WORKER_ADDR)),
            bag::new(ctx),
        );

        validator::validate_metadata(&metadata);
        test_utils::destroy(metadata);
        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = validator::EMetadataInvalidPrimaryAddr)]
    fun test_metadata_invalid_consensus_addr() {
        let scenario_val = test_scenario::begin(VALID_ADDRESS);
        let ctx = test_scenario::ctx(&mut scenario_val);
        let metadata = validator::new_metadata(
            VALID_ADDRESS,
            VALID_PUBKEY,
            VALID_NET_PUBKEY,
            VALID_WORKER_PUBKEY,
            PROOF_OF_POSSESSION,
            string::from_ascii(ascii::string(b"Validator1")),
            string::from_ascii(ascii::string(b"Validator1")),
            url::new_unsafe_from_bytes(b"image_url1"),
            url::new_unsafe_from_bytes(b"project_url1"),
            string::from_ascii(ascii::string(VALID_NET_ADDR)),
            string::from_ascii(ascii::string(VALID_P2P_ADDR)),
            string::from_ascii(ascii::string(b"42")),
            string::from_ascii(ascii::string(VALID_WORKER_ADDR)),
            bag::new(ctx),
        );

        validator::validate_metadata(&metadata);
        test_utils::destroy(metadata);
        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = validator::EMetadataInvalidWorkerAddr)]
    fun test_metadata_invalid_worker_addr() {
        let scenario_val = test_scenario::begin(VALID_ADDRESS);
        let ctx = test_scenario::ctx(&mut scenario_val);
        let metadata = validator::new_metadata(
            VALID_ADDRESS,
            VALID_PUBKEY,
            VALID_NET_PUBKEY,
            VALID_WORKER_PUBKEY,
            PROOF_OF_POSSESSION,
            string::from_ascii(ascii::string(b"Validator1")),
            string::from_ascii(ascii::string(b"Validator1")),
            url::new_unsafe_from_bytes(b"image_url1"),
            url::new_unsafe_from_bytes(b"project_url1"),
            string::from_ascii(ascii::string(VALID_NET_ADDR)),
            string::from_ascii(ascii::string(VALID_P2P_ADDR)),
            string::from_ascii(ascii::string(VALID_CONSENSUS_ADDR)),
            string::from_ascii(ascii::string(b"42")),
            bag::new(ctx),
        );

        validator::validate_metadata(&metadata);
        test_utils::destroy(metadata);
        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_validator_update_metadata_ok() {
        let sender = VALID_ADDRESS;
        let scenario_val = test_scenario::begin(sender);
        let scenario = &mut scenario_val;
        let ctx = test_scenario::ctx(scenario);
        let new_protocol_pub_key = x"96d19c53f1bee2158c3fcfb5bb2f06d3a8237667529d2d8f0fbb22fe5c3b3e64748420b4103674490476d98530d063271222d2a59b0f7932909cc455a30f00c69380e6885375e94243f7468e9563aad29330aca7ab431927540e9508888f0e1c";
        let new_pop = x"a8a0bcaf04e13565914eb22fa9f27a76f297db04446860ee2b923d10224cedb130b30783fb60b12556e7fc50e5b57a86";
        let new_worker_pub_key = vector[115, 220, 238, 151, 134, 159, 173, 41, 80, 2, 66, 196, 61, 17, 191, 76, 103, 39, 246, 127, 171, 85, 19, 235, 210, 106, 97, 97, 116, 48, 244, 191];
        let new_network_pub_key = vector[149, 128, 161, 13, 11, 183, 96, 45, 89, 20, 188, 205, 26, 127, 147, 254, 184, 229, 184, 102, 64, 170, 104, 29, 191, 171, 91, 99, 58, 178, 41, 156];

        let validator = get_test_validator(ctx);

        test_scenario::next_tx(scenario, sender);
        {
            validator::update_next_epoch_network_address(&mut validator, b"/ip4/192.168.1.1/tcp/80");
            validator::update_next_epoch_p2p_address(&mut validator, b"/ip4/192.168.1.1/udp/80");
            validator::update_next_epoch_primary_address(&mut validator, b"/ip4/192.168.1.1/udp/80");
            validator::update_next_epoch_worker_address(&mut validator, b"/ip4/192.168.1.1/udp/80");
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

            validator::update_name(&mut validator, b"new_name");
            validator::update_description(&mut validator, b"new_desc");
            validator::update_image_url(&mut validator, b"new_image_url");
            validator::update_project_url(&mut validator, b"new_proj_url");
        };

        test_scenario::next_tx(scenario, sender);
        {
            // Current epoch
            assert!(validator::name(&mut validator) == &string::from_ascii(ascii::string(b"new_name")), 0);
            assert!(validator::description(&mut validator) == &string::from_ascii(ascii::string(b"new_desc")), 0);
            assert!(validator::image_url(&mut validator) == &url::new_unsafe_from_bytes(b"new_image_url"), 0);
            assert!(validator::project_url(&mut validator) == &url::new_unsafe_from_bytes(b"new_proj_url"), 0);
            assert!(validator::network_address(&validator) == &string::from_ascii(ascii::string(VALID_NET_ADDR)), 0);
            assert!(validator::p2p_address(&validator) == &string::from_ascii(ascii::string(VALID_P2P_ADDR)), 0);
            assert!(validator::primary_address(&validator) == &string::from_ascii(ascii::string(VALID_CONSENSUS_ADDR)), 0);
            assert!(validator::worker_address(&validator) == &string::from_ascii(ascii::string(VALID_WORKER_ADDR)), 0);
            assert!(validator::protocol_pubkey_bytes(&validator) == &VALID_PUBKEY, 0);
            assert!(validator::proof_of_possession(&validator) == &PROOF_OF_POSSESSION, 0);
            assert!(validator::network_pubkey_bytes(&validator) == &VALID_NET_PUBKEY, 0);
            assert!(validator::worker_pubkey_bytes(&validator) == &VALID_WORKER_PUBKEY, 0);

            // Next epoch
            assert!(validator::next_epoch_network_address(&validator) == &option::some(string::from_ascii(ascii::string(b"/ip4/192.168.1.1/tcp/80"))), 0);
            assert!(validator::next_epoch_p2p_address(&validator) == &option::some(string::from_ascii(ascii::string(b"/ip4/192.168.1.1/udp/80"))), 0);
            assert!(validator::next_epoch_primary_address(&validator) == &option::some(string::from_ascii(ascii::string(b"/ip4/192.168.1.1/udp/80"))), 0);
            assert!(validator::next_epoch_worker_address(&validator) == &option::some(string::from_ascii(ascii::string(b"/ip4/192.168.1.1/udp/80"))), 0);
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

    #[expected_failure(abort_code = sui_system::validator::EInvalidProofOfPossession)]
    #[test]
    fun test_validator_update_metadata_invalid_proof_of_possession() {
        let (sender, scenario, validator) = set_up();

        test_scenario::next_tx(&mut scenario, sender);
        {
            validator::update_next_epoch_protocol_pubkey(
                &mut validator,
                x"96d19c53f1bee2158c3fcfb5bb2f06d3a8237667529d2d8f0fbb22fe5c3b3e64748420b4103674490476d98530d063271222d2a59b0f7932909cc455a30f00c69380e6885375e94243f7468e9563aad29330aca7ab431927540e9508888f0e1c",
                // This is an invalid proof of possession, so we abort
                x"8b9794dfd11b88e16ba8f6a4a2c1e7580738dce2d6910ee594bebd88297b22ae8c34d1ee3f5a081159d68e076ef5d300");
        };

        tear_down(validator, scenario);
    }

    #[expected_failure(abort_code = sui_system::validator::EMetadataInvalidNetPubkey)]
    #[test]
    fun test_validator_update_metadata_invalid_network_key() {
        let (sender, scenario, validator) = set_up();

        test_scenario::next_tx(&mut scenario, sender);
        {
            validator::update_next_epoch_network_pubkey(
                &mut validator,
                x"beef",
            );
        };

        tear_down(validator, scenario);
    }

    #[expected_failure(abort_code = sui_system::validator::EMetadataInvalidWorkerPubkey)]
    #[test]
    fun test_validator_update_metadata_invalid_worker_key() {
        let (sender, scenario, validator) = set_up();

        test_scenario::next_tx(&mut scenario, sender);
        {
            validator::update_next_epoch_worker_pubkey(
                &mut validator,
                x"beef",
            );
        };

        tear_down(validator, scenario);
    }

    #[expected_failure(abort_code = sui_system::validator::EMetadataInvalidNetAddr)]
    #[test]
    fun test_validator_update_metadata_invalid_network_addr() {
        let (sender, scenario, validator) = set_up();

        test_scenario::next_tx(&mut scenario, sender);
        {
            validator::update_next_epoch_network_address(
                &mut validator,
                b"beef",
            );
        };

        tear_down(validator, scenario);
    }

    #[expected_failure(abort_code = sui_system::validator::EMetadataInvalidPrimaryAddr)]
    #[test]
    fun test_validator_update_metadata_invalid_primary_addr() {
        let (sender, scenario, validator) = set_up();

        test_scenario::next_tx(&mut scenario, sender);
        {
            validator::update_next_epoch_primary_address(
                &mut validator,
                b"beef",
            );
        };

        tear_down(validator, scenario);
    }

    #[expected_failure(abort_code = sui_system::validator::EMetadataInvalidWorkerAddr)]
    #[test]
    fun test_validator_update_metadata_invalid_worker_addr() {
        let (sender, scenario, validator) = set_up();

        test_scenario::next_tx(&mut scenario, sender);
        {
            validator::update_next_epoch_worker_address(
                &mut validator,
                b"beef",
            );
        };

        tear_down(validator, scenario);
    }

    #[expected_failure(abort_code = sui_system::validator::EMetadataInvalidP2pAddr)]
    #[test]
    fun test_validator_update_metadata_invalid_p2p_address() {
        let (sender, scenario, validator) = set_up();

        test_scenario::next_tx(&mut scenario, sender);
        {
            validator::update_next_epoch_p2p_address(
                &mut validator,
                b"beef",
            );
        };

        tear_down(validator, scenario);
    }

    #[expected_failure(abort_code = sui_system::validator::EValidatorMetadataExceedingLengthLimit)]
    #[test]
    fun test_validator_update_metadata_primary_address_too_long() {
        let (sender, scenario, validator) = set_up();

        test_scenario::next_tx(&mut scenario, sender);
        {
            validator::update_next_epoch_primary_address(
                &mut validator,
                // 257 bytes but limit is 256 bytes
                TOO_LONG_257_BYTES,
            );
        };

        tear_down(validator, scenario);
    }

    #[expected_failure(abort_code = sui_system::validator::EValidatorMetadataExceedingLengthLimit)]
    #[test]
    fun test_validator_update_metadata_net_address_too_long() {
        let (sender, scenario, validator) = set_up();

        test_scenario::next_tx(&mut scenario, sender);
        {
            validator::update_next_epoch_network_address(
                &mut validator,
                // 257 bytes but limit is 256 bytes
                TOO_LONG_257_BYTES,
            );
        };

        tear_down(validator, scenario);
    }


    #[expected_failure(abort_code = sui_system::validator::EValidatorMetadataExceedingLengthLimit)]
    #[test]
    fun test_validator_update_metadata_worker_address_too_long() {
        let (sender, scenario, validator) = set_up();
        test_scenario::next_tx(&mut scenario, sender);
        {
            validator::update_next_epoch_worker_address(
                &mut validator,
                // 257 bytes but limit is 256 bytes
                TOO_LONG_257_BYTES,
            );
        };

        tear_down(validator, scenario);
    }

    #[expected_failure(abort_code = sui_system::validator::EValidatorMetadataExceedingLengthLimit)]
    #[test]
    fun test_validator_update_metadata_p2p_address_too_long() {
        let (sender, scenario, validator) = set_up();

        test_scenario::next_tx(&mut scenario, sender);
        {
            validator::update_next_epoch_p2p_address(
                &mut validator,
                // 257 bytes but limit is 256 bytes
                TOO_LONG_257_BYTES,
            );
        };

        tear_down(validator, scenario);
    }

    #[expected_failure(abort_code = sui_system::validator::EValidatorMetadataExceedingLengthLimit)]
    #[test]
    fun test_validator_update_name_too_long() {
        let (sender, scenario, validator) = set_up();

        test_scenario::next_tx(&mut scenario, sender);
        {
            validator::update_name(
                &mut validator,
                // 257 bytes but limit is 256 bytes
                TOO_LONG_257_BYTES,
            );
        };
        tear_down(validator, scenario);
    }

    #[expected_failure(abort_code = sui_system::validator::EValidatorMetadataExceedingLengthLimit)]
    #[test]
    fun test_validator_update_description_too_long() {
        let (sender, scenario, validator) = set_up();

        test_scenario::next_tx(&mut scenario, sender);
        {
            validator::update_description(
                &mut validator,
                // 257 bytes but limit is 256 bytes
                TOO_LONG_257_BYTES,
            );
        };
        tear_down(validator, scenario);
    }

    #[expected_failure(abort_code = sui_system::validator::EValidatorMetadataExceedingLengthLimit)]
    #[test]
    fun test_validator_update_project_url_too_long() {
        let (sender, scenario, validator) = set_up();

        test_scenario::next_tx(&mut scenario, sender);
        {
            validator::update_project_url(
                &mut validator,
                // 257 bytes but limit is 256 bytes
                TOO_LONG_257_BYTES,
            );
        };
        tear_down(validator, scenario);
    }

    #[expected_failure(abort_code = sui_system::validator::EValidatorMetadataExceedingLengthLimit)]
    #[test]
    fun test_validator_update_image_url_too_long() {
        let (sender, scenario, validator) = set_up();

        test_scenario::next_tx(&mut scenario, sender);
        {
            validator::update_image_url(
                &mut validator,
                // 257 bytes but limit is 256 bytes
                TOO_LONG_257_BYTES,
            );
        };
        tear_down(validator, scenario);
    }

    fun set_up(): (address, test_scenario::Scenario, validator::Validator) {
        let sender = VALID_ADDRESS;
        let scenario_val = test_scenario::begin(sender);
        let ctx = test_scenario::ctx(&mut scenario_val);
        let validator = get_test_validator(ctx);
        (sender, scenario_val, validator)
    }

    fun tear_down(validator: validator::Validator, scenario: test_scenario::Scenario) {
        test_utils::destroy(validator);
        test_scenario::end(scenario);
    }
}
