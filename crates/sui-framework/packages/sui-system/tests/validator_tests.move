// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui_system::validator_tests {
    use sui::bag;
    use sui::balance;
    use sui::coin::{Self, Coin};
    use sui::sui::SUI;
    use sui::test_scenario;
    use sui::test_utils;
    use sui::url;
    use sui_system::staking_pool::StakedSui;
    use sui_system::validator::{Self, Validator};

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
        let init_stake = coin::mint_for_testing(10_000_000_000, ctx).into_balance();
        let mut validator = validator::new(
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

        validator.request_add_stake_at_genesis(
            init_stake,
            VALID_ADDRESS,
            ctx
        );

        validator.activate(0);

        validator
    }

    #[test]
    fun test_validator_owner_flow() {
        let sender = VALID_ADDRESS;
        let mut scenario_val = test_scenario::begin(sender);
        let scenario = &mut scenario_val;
        {
            let ctx = scenario.ctx();

            let validator = get_test_validator(ctx);
            assert!(validator.total_stake_amount() == 10_000_000_000);
            assert!(validator.sui_address() == sender);

            test_utils::destroy(validator);
        };

        // Check that after destroy, the original stake still exists.
         scenario.next_tx(sender);
         {
             let stake = scenario.take_from_sender<StakedSui>();
             assert!(stake.amount() == 10_000_000_000);
             scenario.return_to_sender(stake);
         };
        scenario_val.end();
    }

    #[test]
    fun test_pending_validator_flow() {
        let sender = VALID_ADDRESS;
        let mut scenario_val = test_scenario::begin(sender);
        let scenario = &mut scenario_val;
        let ctx = scenario.ctx();

        let mut validator = get_test_validator(ctx);
        scenario.next_tx(sender);
        {
            let ctx = scenario.ctx();
            let new_stake = coin::mint_for_testing(30_000_000_000, ctx).into_balance();
            let stake = validator.request_add_stake(new_stake, sender, ctx);
            transfer::public_transfer(stake, sender);

            assert!(validator.total_stake() == 10_000_000_000);
            assert!(validator.pending_stake_amount() == 30_000_000_000);
        };

        scenario.next_tx(sender);
        {
            let coin_ids = scenario.ids_for_sender<StakedSui>();
            let stake = scenario.take_from_sender_by_id<StakedSui>(coin_ids[0]);
            let ctx = scenario.ctx();
            let withdrawn_balance = validator.request_withdraw_stake(stake, ctx);
            transfer::public_transfer(withdrawn_balance.into_coin(ctx), sender);

            assert!(validator.total_stake() == 10_000_000_000);
            assert!(validator.pending_stake_amount() == 30_000_000_000);
            assert!(validator.pending_stake_withdraw_amount() == 10_000_000_000);

            validator.deposit_stake_rewards(balance::zero());

            // Calling `process_pending_stakes_and_withdraws` will withdraw the coin and transfer to sender.
            validator.process_pending_stakes_and_withdraws(ctx);

            assert!(validator.total_stake() == 30_000_000_000);
            assert!(validator.pending_stake_amount() == 0);
            assert!(validator.pending_stake_withdraw_amount() == 0);
        };

        scenario.next_tx(sender);
        {
            let coin_ids = scenario.ids_for_sender<Coin<SUI>>();
            let withdraw = scenario.take_from_sender_by_id<Coin<SUI>>(coin_ids[0]);
            assert!(withdraw.value() == 10_000_000_000);
            scenario.return_to_sender(withdraw);
        };

        test_utils::destroy(validator);
        scenario_val.end();
    }

    #[test]
    fun test_metadata() {
        let mut scenario_val = test_scenario::begin(VALID_ADDRESS);
        let ctx = scenario_val.ctx();
        let metadata = validator::new_metadata(
            VALID_ADDRESS,
            VALID_PUBKEY,
            VALID_NET_PUBKEY,
            VALID_WORKER_PUBKEY,
            PROOF_OF_POSSESSION,
            b"Validator1".to_string(),
            b"Validator1".to_string(),
            url::new_unsafe_from_bytes(b"image_url1"),
            url::new_unsafe_from_bytes(b"project_url1"),
            VALID_NET_ADDR.to_string(),
            VALID_P2P_ADDR.to_string(),
            VALID_CONSENSUS_ADDR.to_string(),
            VALID_WORKER_ADDR.to_string(),
            bag::new(ctx),
        );

        validator::validate_metadata(&metadata);
        test_utils::destroy(metadata);
        scenario_val.end();
    }

    #[test]
    #[expected_failure(abort_code = validator::EMetadataInvalidPubkey)]
    fun test_metadata_invalid_pubkey() {
        let mut scenario_val = test_scenario::begin(VALID_ADDRESS);
        let ctx = scenario_val.ctx();
        let metadata = validator::new_metadata(
            VALID_ADDRESS,
            vector[42],
            VALID_NET_PUBKEY,
            VALID_WORKER_PUBKEY,
            PROOF_OF_POSSESSION,
            b"Validator1".to_string(),
            b"Validator1".to_string(),
            url::new_unsafe_from_bytes(b"image_url1"),
            url::new_unsafe_from_bytes(b"project_url1"),
            VALID_NET_ADDR.to_string(),
            VALID_P2P_ADDR.to_string(),
            VALID_CONSENSUS_ADDR.to_string(),
            VALID_WORKER_ADDR.to_string(),
            bag::new(ctx),
        );

        validator::validate_metadata(&metadata);
        test_utils::destroy(metadata);
        scenario_val.end();
    }

    #[test]
    #[expected_failure(abort_code = validator::EMetadataInvalidNetPubkey)]
    fun test_metadata_invalid_net_pubkey() {
        let mut scenario_val = test_scenario::begin(VALID_ADDRESS);
        let ctx = scenario_val.ctx();
        let metadata = validator::new_metadata(
            VALID_ADDRESS,
            VALID_PUBKEY,
            vector[42],
            VALID_WORKER_PUBKEY,
            PROOF_OF_POSSESSION,
            b"Validator1".to_string(),
            b"Validator1".to_string(),
            url::new_unsafe_from_bytes(b"image_url1"),
            url::new_unsafe_from_bytes(b"project_url1"),
            VALID_NET_ADDR.to_string(),
            VALID_P2P_ADDR.to_string(),
            VALID_CONSENSUS_ADDR.to_string(),
            VALID_WORKER_ADDR.to_string(),
            bag::new(ctx),
        );

        validator::validate_metadata(&metadata);
        test_utils::destroy(metadata);
        scenario_val.end();
    }

    #[test]
    #[expected_failure(abort_code = validator::EMetadataInvalidWorkerPubkey)]
    fun test_metadata_invalid_worker_pubkey() {
        let mut scenario_val = test_scenario::begin(VALID_ADDRESS);
        let ctx = scenario_val.ctx();
        let metadata = validator::new_metadata(
            VALID_ADDRESS,
            VALID_PUBKEY,
            VALID_NET_PUBKEY,
            vector[42],
            PROOF_OF_POSSESSION,
            b"Validator1".to_string(),
            b"Validator1".to_string(),
            url::new_unsafe_from_bytes(b"image_url1"),
            url::new_unsafe_from_bytes(b"project_url1"),
            VALID_NET_ADDR.to_string(),
            VALID_P2P_ADDR.to_string(),
            VALID_CONSENSUS_ADDR.to_string(),
            VALID_WORKER_ADDR.to_string(),
            bag::new(ctx),
        );

        validator::validate_metadata(&metadata);
        test_utils::destroy(metadata);
        scenario_val.end();
    }

    #[test]
    #[expected_failure(abort_code = validator::EMetadataInvalidNetAddr)]
    fun test_metadata_invalid_net_addr() {
        let mut scenario_val = test_scenario::begin(VALID_ADDRESS);
        let ctx = scenario_val.ctx();
        let metadata = validator::new_metadata(
            VALID_ADDRESS,
            VALID_PUBKEY,
            VALID_NET_PUBKEY,
            VALID_WORKER_PUBKEY,
            PROOF_OF_POSSESSION,
            b"Validator1".to_string(),
            b"Validator1".to_string(),
            url::new_unsafe_from_bytes(b"image_url1"),
            url::new_unsafe_from_bytes(b"project_url1"),
            b"42".to_string(),
            VALID_P2P_ADDR.to_string(),
            VALID_CONSENSUS_ADDR.to_string(),
            VALID_WORKER_ADDR.to_string(),
            bag::new(ctx),
        );

        validator::validate_metadata(&metadata);
        test_utils::destroy(metadata);
        scenario_val.end();
    }

    #[test]
    #[expected_failure(abort_code = validator::EMetadataInvalidP2pAddr)]
    fun test_metadata_invalid_p2p_addr() {
        let mut scenario_val = test_scenario::begin(VALID_ADDRESS);
        let ctx = scenario_val.ctx();
        let metadata = validator::new_metadata(
            VALID_ADDRESS,
            VALID_PUBKEY,
            VALID_NET_PUBKEY,
            VALID_WORKER_PUBKEY,
            PROOF_OF_POSSESSION,
            b"Validator1".to_string(),
            b"Validator1".to_string(),
            url::new_unsafe_from_bytes(b"image_url1"),
            url::new_unsafe_from_bytes(b"project_url1"),
            VALID_NET_ADDR.to_string(),
            b"42".to_string(),
            VALID_CONSENSUS_ADDR.to_string(),
            VALID_WORKER_ADDR.to_string(),
            bag::new(ctx),
        );

        validator::validate_metadata(&metadata);
        test_utils::destroy(metadata);
        scenario_val.end();
    }

    #[test]
    #[expected_failure(abort_code = validator::EMetadataInvalidPrimaryAddr)]
    fun test_metadata_invalid_consensus_addr() {
        let mut scenario_val = test_scenario::begin(VALID_ADDRESS);
        let ctx = scenario_val.ctx();
        let metadata = validator::new_metadata(
            VALID_ADDRESS,
            VALID_PUBKEY,
            VALID_NET_PUBKEY,
            VALID_WORKER_PUBKEY,
            PROOF_OF_POSSESSION,
            b"Validator1".to_string(),
            b"Validator1".to_string(),
            url::new_unsafe_from_bytes(b"image_url1"),
            url::new_unsafe_from_bytes(b"project_url1"),
            VALID_NET_ADDR.to_string(),
            VALID_P2P_ADDR.to_string(),
            b"42".to_string(),
            VALID_WORKER_ADDR.to_string(),
            bag::new(ctx),
        );

        validator::validate_metadata(&metadata);
        test_utils::destroy(metadata);
        scenario_val.end();
    }

    #[test]
    #[expected_failure(abort_code = validator::EMetadataInvalidWorkerAddr)]
    fun test_metadata_invalid_worker_addr() {
        let mut scenario_val = test_scenario::begin(VALID_ADDRESS);
        let ctx = scenario_val.ctx();
        let metadata = validator::new_metadata(
            VALID_ADDRESS,
            VALID_PUBKEY,
            VALID_NET_PUBKEY,
            VALID_WORKER_PUBKEY,
            PROOF_OF_POSSESSION,
            b"Validator1".to_string(),
            b"Validator1".to_string(),
            url::new_unsafe_from_bytes(b"image_url1"),
            url::new_unsafe_from_bytes(b"project_url1"),
            VALID_NET_ADDR.to_string(),
            VALID_P2P_ADDR.to_string(),
            VALID_CONSENSUS_ADDR.to_string(),
            b"42".to_string(),
            bag::new(ctx),
        );

        validator::validate_metadata(&metadata);
        test_utils::destroy(metadata);
        scenario_val.end();
    }

    #[test, allow(implicit_const_copy)]
    fun test_validator_update_metadata_ok() {
        let sender = VALID_ADDRESS;
        let mut scenario_val = test_scenario::begin(sender);
        let scenario = &mut scenario_val;
        let ctx = scenario.ctx();
        let new_protocol_pub_key = x"96d19c53f1bee2158c3fcfb5bb2f06d3a8237667529d2d8f0fbb22fe5c3b3e64748420b4103674490476d98530d063271222d2a59b0f7932909cc455a30f00c69380e6885375e94243f7468e9563aad29330aca7ab431927540e9508888f0e1c";
        let new_pop = x"a8a0bcaf04e13565914eb22fa9f27a76f297db04446860ee2b923d10224cedb130b30783fb60b12556e7fc50e5b57a86";
        let new_worker_pub_key = vector[115, 220, 238, 151, 134, 159, 173, 41, 80, 2, 66, 196, 61, 17, 191, 76, 103, 39, 246, 127, 171, 85, 19, 235, 210, 106, 97, 97, 116, 48, 244, 191];
        let new_network_pub_key = vector[149, 128, 161, 13, 11, 183, 96, 45, 89, 20, 188, 205, 26, 127, 147, 254, 184, 229, 184, 102, 64, 170, 104, 29, 191, 171, 91, 99, 58, 178, 41, 156];

        let mut validator = get_test_validator(ctx);

        scenario.next_tx(sender);
        {
            validator.update_next_epoch_network_address(b"/ip4/192.168.1.1/tcp/80");
            validator.update_next_epoch_p2p_address(b"/ip4/192.168.1.1/udp/80");
            validator.update_next_epoch_primary_address(b"/ip4/192.168.1.1/udp/80");
            validator.update_next_epoch_worker_address(b"/ip4/192.168.1.1/udp/80");
            validator.update_next_epoch_protocol_pubkey(new_protocol_pub_key, new_pop);
            validator.update_next_epoch_worker_pubkey( new_worker_pub_key);
            validator.update_next_epoch_network_pubkey(new_network_pub_key);

            validator.update_name(b"new_name");
            validator.update_description(b"new_desc");
            validator.update_image_url(b"new_image_url");
            validator.update_project_url(b"new_proj_url");
        };

        scenario.next_tx(sender);
        {
            // Current epoch
            assert!(validator.name() == &b"new_name".to_string());
            assert!(validator.description() == &b"new_desc".to_string());
            assert!(validator.image_url() == &url::new_unsafe_from_bytes(b"new_image_url"));
            assert!(validator.project_url() == &url::new_unsafe_from_bytes(b"new_proj_url"));
            assert!(validator.network_address() == &VALID_NET_ADDR.to_string());
            assert!(validator.p2p_address() == &VALID_P2P_ADDR.to_string());
            assert!(validator.primary_address() == &VALID_CONSENSUS_ADDR.to_string());
            assert!(validator.worker_address() == &VALID_WORKER_ADDR.to_string());
            assert!(validator.protocol_pubkey_bytes() == &VALID_PUBKEY);
            assert!(validator.proof_of_possession() == &PROOF_OF_POSSESSION);
            assert!(validator.network_pubkey_bytes() == &VALID_NET_PUBKEY);
            assert!(validator.worker_pubkey_bytes() == &VALID_WORKER_PUBKEY);

            // Next epoch
            assert!(validator.next_epoch_network_address() == &option::some(b"/ip4/192.168.1.1/tcp/80".to_string()));
            assert!(validator.next_epoch_p2p_address() == &option::some(b"/ip4/192.168.1.1/udp/80".to_string()));
            assert!(validator.next_epoch_primary_address() == &option::some(b"/ip4/192.168.1.1/udp/80".to_string()));
            assert!(validator.next_epoch_worker_address() == &option::some(b"/ip4/192.168.1.1/udp/80".to_string()));
            assert!(
                validator.next_epoch_protocol_pubkey_bytes() == &option::some(new_protocol_pub_key),
                0
            );
            assert!(
                validator.next_epoch_proof_of_possession() == &option::some(new_pop),
                0
            );
            assert!(
                validator.next_epoch_worker_pubkey_bytes() == &option::some(new_worker_pub_key),
                0
            );
            assert!(
                validator.next_epoch_network_pubkey_bytes() == &option::some(new_network_pub_key),
                0
            );
        };

        test_utils::destroy(validator);
        scenario_val.end();
    }

    #[expected_failure(abort_code = sui_system::validator::EInvalidProofOfPossession)]
    #[test]
    fun test_validator_update_metadata_invalid_proof_of_possession() {
        let (sender, mut scenario, mut validator) = set_up();

        scenario.next_tx(sender);
        {
            validator.update_next_epoch_protocol_pubkey(
                x"96d19c53f1bee2158c3fcfb5bb2f06d3a8237667529d2d8f0fbb22fe5c3b3e64748420b4103674490476d98530d063271222d2a59b0f7932909cc455a30f00c69380e6885375e94243f7468e9563aad29330aca7ab431927540e9508888f0e1c",
                // This is an invalid proof of possession, so we abort
                x"8b9794dfd11b88e16ba8f6a4a2c1e7580738dce2d6910ee594bebd88297b22ae8c34d1ee3f5a081159d68e076ef5d300");
        };

        tear_down(validator, scenario);
    }

    #[expected_failure(abort_code = sui_system::validator::EMetadataInvalidNetPubkey)]
    #[test]
    fun test_validator_update_metadata_invalid_network_key() {
        let (sender, mut scenario, mut validator) = set_up();

        scenario.next_tx(sender);
        {
            validator.update_next_epoch_network_pubkey(x"beef");
        };

        tear_down(validator, scenario);
    }

    #[expected_failure(abort_code = sui_system::validator::EMetadataInvalidWorkerPubkey)]
    #[test]
    fun test_validator_update_metadata_invalid_worker_key() {
        let (sender, mut scenario, mut validator) = set_up();

        scenario.next_tx(sender);
        {
            validator.update_next_epoch_worker_pubkey(x"beef");
        };

        tear_down(validator, scenario);
    }

    #[expected_failure(abort_code = sui_system::validator::EMetadataInvalidNetAddr)]
    #[test]
    fun test_validator_update_metadata_invalid_network_addr() {
        let (sender, mut scenario, mut validator) = set_up();

        scenario.next_tx(sender);
        {
            validator.update_next_epoch_network_address(b"beef");
        };

        tear_down(validator, scenario);
    }

    #[expected_failure(abort_code = sui_system::validator::EMetadataInvalidPrimaryAddr)]
    #[test]
    fun test_validator_update_metadata_invalid_primary_addr() {
        let (sender, mut scenario, mut validator) = set_up();

        scenario.next_tx(sender);
        {
            validator.update_next_epoch_primary_address(b"beef");
        };

        tear_down(validator, scenario);
    }

    #[expected_failure(abort_code = sui_system::validator::EMetadataInvalidWorkerAddr)]
    #[test]
    fun test_validator_update_metadata_invalid_worker_addr() {
        let (sender, mut scenario, mut validator) = set_up();

        scenario.next_tx(sender);
        {
            validator.update_next_epoch_worker_address(b"beef");
        };

        tear_down(validator, scenario);
    }

    #[expected_failure(abort_code = sui_system::validator::EMetadataInvalidP2pAddr)]
    #[test]
    fun test_validator_update_metadata_invalid_p2p_address() {
        let (sender, mut scenario, mut validator) = set_up();

        scenario.next_tx(sender);
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
        let (sender, mut scenario, mut validator) = set_up();

        scenario.next_tx(sender);
        {
            validator.update_next_epoch_primary_address(
                // 257 bytes but limit is 256 bytes
                TOO_LONG_257_BYTES,
            );
        };

        tear_down(validator, scenario);
    }

    #[expected_failure(abort_code = sui_system::validator::EValidatorMetadataExceedingLengthLimit)]
    #[test]
    fun test_validator_update_metadata_net_address_too_long() {
        let (sender, mut scenario, mut validator) = set_up();

        scenario.next_tx(sender);
        {
            validator.update_next_epoch_network_address(
                // 257 bytes but limit is 256 bytes
                TOO_LONG_257_BYTES,
            );
        };

        tear_down(validator, scenario);
    }


    #[expected_failure(abort_code = sui_system::validator::EValidatorMetadataExceedingLengthLimit)]
    #[test]
    fun test_validator_update_metadata_worker_address_too_long() {
        let (sender, mut scenario, mut validator) = set_up();
        scenario.next_tx(sender);
        {
            validator.update_next_epoch_worker_address(
                // 257 bytes but limit is 256 bytes
                TOO_LONG_257_BYTES,
            );
        };

        tear_down(validator, scenario);
    }

    #[expected_failure(abort_code = sui_system::validator::EValidatorMetadataExceedingLengthLimit)]
    #[test]
    fun test_validator_update_metadata_p2p_address_too_long() {
        let (sender, mut scenario, mut validator) = set_up();

        scenario.next_tx(sender);
        {
            validator.update_next_epoch_p2p_address(
                // 257 bytes but limit is 256 bytes
                TOO_LONG_257_BYTES,
            );
        };

        tear_down(validator, scenario);
    }

    #[expected_failure(abort_code = sui_system::validator::EValidatorMetadataExceedingLengthLimit)]
    #[test]
    fun test_validator_update_name_too_long() {
        let (sender, mut scenario, mut validator) = set_up();

        scenario.next_tx(sender);
        {
            validator.update_name(
                // 257 bytes but limit is 256 bytes
                TOO_LONG_257_BYTES,
            );
        };
        tear_down(validator, scenario);
    }

    #[expected_failure(abort_code = sui_system::validator::EValidatorMetadataExceedingLengthLimit)]
    #[test]
    fun test_validator_update_description_too_long() {
        let (sender, mut scenario, mut validator) = set_up();

        scenario.next_tx(sender);
        {
            validator.update_description(
                // 257 bytes but limit is 256 bytes
                TOO_LONG_257_BYTES,
            );
        };
        tear_down(validator, scenario);
    }

    #[expected_failure(abort_code = sui_system::validator::EValidatorMetadataExceedingLengthLimit)]
    #[test]
    fun test_validator_update_project_url_too_long() {
        let (sender, mut scenario, mut validator) = set_up();

        scenario.next_tx(sender);
        {
            validator.update_project_url(
                // 257 bytes but limit is 256 bytes
                TOO_LONG_257_BYTES,
            );
        };
        tear_down(validator, scenario);
    }

    #[expected_failure(abort_code = sui_system::validator::EValidatorMetadataExceedingLengthLimit)]
    #[test]
    fun test_validator_update_image_url_too_long() {
        let (sender, mut scenario, mut validator) = set_up();

        scenario.next_tx(sender);
        {
            validator.update_image_url(
                // 257 bytes but limit is 256 bytes
                TOO_LONG_257_BYTES,
            );
        };
        tear_down(validator, scenario);
    }

    fun set_up(): (address, test_scenario::Scenario, validator::Validator) {
        let sender = VALID_ADDRESS;
        let mut scenario_val = test_scenario::begin(sender);
        let ctx = scenario_val.ctx();
        let validator = get_test_validator(ctx);
        (sender, scenario_val, validator)
    }

    fun tear_down(validator: validator::Validator, scenario: test_scenario::Scenario) {
        test_utils::destroy(validator);
        scenario.end();
    }
}
