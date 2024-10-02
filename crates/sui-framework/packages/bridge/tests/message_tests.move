// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module bridge::message_tests {
    use bridge::{
        chain_ids,
        message::{
            BridgeMessage, blocklist_validator_addresses,
            create_add_tokens_on_sui_message, create_blocklist_message,
            create_emergency_op_message, create_token_bridge_message,
            create_update_asset_price_message, create_update_bridge_limit_message,
            deserialize_message_test_only, emergency_op_pause, emergency_op_unpause,
            extract_add_tokens_on_sui, extract_blocklist_payload, extract_token_bridge_payload,
            extract_update_asset_price, extract_update_bridge_limit, make_add_token_on_sui,
            make_generic_message, make_payload, peel_u64_be_for_testing, reverse_bytes_test,
            serialize_message, set_payload,
            update_asset_price_payload_token_id, update_bridge_limit_payload_limit,
            update_bridge_limit_payload_receiving_chain,
            update_bridge_limit_payload_sending_chain,
        },
        treasury::{Self, BTC, ETH, USDC},
    };
    use std::ascii;
    use sui::{
        address, balance,
        coin::{Self, Coin},
        hex, test_scenario,
        test_utils::{assert_eq, destroy},
    };
    use sui::bcs;

    const INVALID_CHAIN: u8 = 42;

    #[test]
    fun test_message_serialization_sui_to_eth() {
        let sender_address = address::from_u256(100);
        let mut scenario = test_scenario::begin(sender_address);
        let ctx = test_scenario::ctx(&mut scenario);
        let coin = coin::mint_for_testing<USDC>(12345, ctx);

        let token_bridge_message = default_token_bridge_message(
            sender_address,
            &coin,
            chain_ids::sui_testnet(),
            chain_ids::eth_sepolia(),
        );

        // Test payload extraction
        let token_payload = make_payload(
            address::to_bytes(sender_address),
            chain_ids::eth_sepolia(),
            hex::decode(b"00000000000000000000000000000000000000c8"),
            3u8,
            balance::value(coin::balance(&coin)),
        );
        let payload = token_bridge_message.extract_token_bridge_payload();
        assert!(payload.token_target_chain() == token_payload.token_target_chain());
        assert!(payload.token_target_address() == token_payload.token_target_address());
        assert!(payload.token_type() == token_payload.token_type());
        assert!(payload.token_amount() == token_payload.token_amount());
        assert!(payload == token_payload);

        // Test message serialization
        let message = serialize_message(token_bridge_message);
        let expected_msg = hex::decode(
            b"0001000000000000000a012000000000000000000000000000000000000000000000000000000000000000640b1400000000000000000000000000000000000000c8030000000000003039",
        );

        assert!(message == expected_msg);
        assert!(token_bridge_message == deserialize_message_test_only(message));

        coin::burn_for_testing(coin);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_message_serialization_eth_to_sui() {
        let address_1 = address::from_u256(100);
        let mut scenario = test_scenario::begin(address_1);
        let ctx = test_scenario::ctx(&mut scenario);

        let coin = coin::mint_for_testing<USDC>(12345, ctx);

        let token_bridge_message = create_token_bridge_message(
            chain_ids::eth_sepolia(), // source chain
            10, // seq_num
            // Eth address is 20 bytes long
            hex::decode(b"00000000000000000000000000000000000000c8"), // eth sender address
            chain_ids::sui_testnet(), // target_chain
            address::to_bytes(address_1), // target address
            3u8, // token_type
            balance::value(coin::balance(&coin)), // amount: u64
        );

        // Test payload extraction
        let token_payload = make_payload(
            hex::decode(b"00000000000000000000000000000000000000c8"),
            chain_ids::sui_testnet(),
            address::to_bytes(address_1),
            3u8,
            balance::value(coin::balance(&coin)),
        );
        assert!(token_bridge_message.extract_token_bridge_payload() == token_payload);


        // Test message serialization
        let message = serialize_message(token_bridge_message);
        let expected_msg = hex::decode(
            b"0001000000000000000a0b1400000000000000000000000000000000000000c801200000000000000000000000000000000000000000000000000000000000000064030000000000003039",
        );
        assert!(message == expected_msg);
        assert!(token_bridge_message == deserialize_message_test_only(message));

        coin::burn_for_testing(coin);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_emergency_op_message_serialization() {
        let emergency_op_message = create_emergency_op_message(
            chain_ids::sui_testnet(), // source chain
            10, // seq_num
            emergency_op_pause(),
        );

        // Test message serialization
        let message = serialize_message(emergency_op_message);
        let expected_msg = hex::decode(b"0201000000000000000a0100");

        assert!(message == expected_msg);
        assert!(emergency_op_message == deserialize_message_test_only(message));
    }

    // Do not change/remove this test, it uses move bytes generated by Rust
    #[test]
    fun test_emergency_op_message_serialization_regression() {
        let emergency_op_message = create_emergency_op_message(
            chain_ids::sui_custom(),
            55, // seq_num
            emergency_op_pause(),
        );

        // Test message serialization
        let message = serialize_message(emergency_op_message);
        let expected_msg = hex::decode(b"020100000000000000370200");

        assert_eq(expected_msg, message);
        assert!(emergency_op_message == deserialize_message_test_only(message));
    }

    #[test]
    fun test_blocklist_message_serialization() {
        let validator_pub_key1 = hex::decode(b"b14d3c4f5fbfbcfb98af2d330000d49c95b93aa7");
        let validator_pub_key2 = hex::decode(b"f7e93cc543d97af6632c9b8864417379dba4bf15");

        let validator_eth_addresses = vector[validator_pub_key1, validator_pub_key2];
        let blocklist_message = create_blocklist_message(
            chain_ids::sui_testnet(), // source chain
            10, // seq_num
            0,
            validator_eth_addresses,
        );
        // Test message serialization
        let message = serialize_message(blocklist_message);

        let expected_msg = hex::decode(
            b"0101000000000000000a010002b14d3c4f5fbfbcfb98af2d330000d49c95b93aa7f7e93cc543d97af6632c9b8864417379dba4bf15",
        );

        assert!(message == expected_msg);
        assert!(blocklist_message == deserialize_message_test_only(message));

        let blocklist = blocklist_message.extract_blocklist_payload();
        assert!(blocklist.blocklist_validator_addresses() == validator_eth_addresses)
    }

    // Do not change/remove this test, it uses move bytes generated by Rust
    #[test]
    fun test_blocklist_message_serialization_regression() {
        let validator_eth_addr_1 = hex::decode(b"68b43fd906c0b8f024a18c56e06744f7c6157c65");
        let validator_eth_addr_2 = hex::decode(b"acaef39832cb995c4e049437a3e2ec6a7bad1ab5");
        // Test 1
        let validator_eth_addresses = vector[validator_eth_addr_1];
        let blocklist_message = create_blocklist_message(
            chain_ids::sui_custom(), // source chain
            129, // seq_num
            0, // blocklist
            validator_eth_addresses,
        );
        // Test message serialization
        let message = serialize_message(blocklist_message);

        let expected_msg = hex::decode(
            b"0101000000000000008102000168b43fd906c0b8f024a18c56e06744f7c6157c65",
        );

        assert_eq(expected_msg, message);
        assert!(blocklist_message == deserialize_message_test_only(message));

        let blocklist = blocklist_message.extract_blocklist_payload();
        assert!(blocklist.blocklist_validator_addresses() == validator_eth_addresses);

        // Test 2
        let validator_eth_addresses = vector[validator_eth_addr_1, validator_eth_addr_2];
        let blocklist_message = create_blocklist_message(
            chain_ids::sui_custom(), // source chain
            68, // seq_num
            1, // unblocklist
            validator_eth_addresses,
        );
        // Test message serialization
        let message = serialize_message(blocklist_message);

        let expected_msg = hex::decode(
            b"0101000000000000004402010268b43fd906c0b8f024a18c56e06744f7c6157c65acaef39832cb995c4e049437a3e2ec6a7bad1ab5",
        );

        assert_eq(expected_msg, message);
        assert!(blocklist_message == deserialize_message_test_only(message));

        let blocklist = blocklist_message.extract_blocklist_payload();
        assert!(blocklist.blocklist_validator_addresses() == validator_eth_addresses)
    }

    #[test]
    fun test_update_bridge_limit_message_serialization() {
        let update_bridge_limit = create_update_bridge_limit_message(
            chain_ids::sui_testnet(), // source chain
            10, // seq_num
            chain_ids::eth_sepolia(),
            1000000000,
        );

        // Test message serialization
        let message = serialize_message(update_bridge_limit);
        let expected_msg = hex::decode(b"0301000000000000000a010b000000003b9aca00");

        assert!(message == expected_msg);
        assert!(update_bridge_limit == deserialize_message_test_only(message));

        let bridge_limit = extract_update_bridge_limit(&update_bridge_limit);
        assert!(
            bridge_limit.update_bridge_limit_payload_receiving_chain()
                == chain_ids::sui_testnet(),
        );
        assert!(
            bridge_limit.update_bridge_limit_payload_sending_chain()
                == chain_ids::eth_sepolia(),
        );
        assert!(bridge_limit.update_bridge_limit_payload_limit() == 1000000000);
    }

    // Do not change/remove this test, it uses move bytes generated by Rust
    #[test]
    fun test_update_bridge_limit_message_serialization_regression() {
        let update_bridge_limit = create_update_bridge_limit_message(
            chain_ids::sui_custom(), // source chain
            15, // seq_num
            chain_ids::eth_custom(),
            10_000_000_000, // 1M USD
        );

        // Test message serialization
        let message = serialize_message(update_bridge_limit);
        let expected_msg = hex::decode(b"0301000000000000000f020c00000002540be400");

        assert_eq(message, expected_msg);
        assert!(update_bridge_limit == deserialize_message_test_only(message));

        let bridge_limit = extract_update_bridge_limit(&update_bridge_limit);
        assert!(
            bridge_limit.update_bridge_limit_payload_receiving_chain()
                == chain_ids::sui_custom(),
        );
        assert!(
            bridge_limit.update_bridge_limit_payload_sending_chain()
                == chain_ids::eth_custom(),
        );
        assert!(bridge_limit.update_bridge_limit_payload_limit() == 10_000_000_000);
    }

    #[test]
    fun test_update_asset_price_message_serialization() {
        let asset_price_message = create_update_asset_price_message(
            2,
            chain_ids::sui_testnet(), // source chain
            10, // seq_num
            12345,
        );

        // Test message serialization
        let message = serialize_message(asset_price_message);
        let expected_msg = hex::decode(b"0401000000000000000a01020000000000003039");
        assert!(message == expected_msg);
        assert!(asset_price_message == deserialize_message_test_only(message));

        let asset_price = extract_update_asset_price(&asset_price_message);

        let mut scenario = test_scenario::begin(@0x1);
        let ctx = test_scenario::ctx(&mut scenario);
        let treasury = treasury::mock_for_test(ctx);

        assert!(
            asset_price.update_asset_price_payload_token_id()
                == treasury::token_id<ETH>(&treasury),
        );
        assert!(asset_price.update_asset_price_payload_new_price() == 12345);

        destroy(treasury);
        test_scenario::end(scenario);
    }

    // Do not change/remove this test, it uses move bytes generated by Rust
    #[test]
    fun test_update_asset_price_message_serialization_regression() {
        let mut scenario = test_scenario::begin(@0x1);
        let ctx = test_scenario::ctx(&mut scenario);
        let treasury = treasury::mock_for_test(ctx);

        let asset_price_message = create_update_asset_price_message(
            treasury.token_id<BTC>(),
            chain_ids::sui_custom(), // source chain
            266, // seq_num
            1_000_000_000, // $100k USD
        );

        // Test message serialization
        let message = serialize_message(asset_price_message);
        let expected_msg = hex::decode(b"0401000000000000010a0201000000003b9aca00");
        assert_eq(expected_msg, message);
        assert!(asset_price_message == deserialize_message_test_only(message));

        let asset_price = extract_update_asset_price(&asset_price_message);

        assert!(
            asset_price.update_asset_price_payload_token_id()
                == treasury::token_id<BTC>(&treasury),
        );
        assert!(asset_price.update_asset_price_payload_new_price() == 1_000_000_000);

        destroy(treasury);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_add_tokens_on_sui_message_serialization() {
        let mut scenario = test_scenario::begin(@0x1);
        let ctx = test_scenario::ctx(&mut scenario);
        let treasury = treasury::mock_for_test(ctx);

        let add_tokens_on_sui_message = create_add_tokens_on_sui_message(
            chain_ids::sui_custom(),
            1, // seq_num
            false, // native_token
            vector[treasury.token_id<BTC>(), treasury.token_id<ETH>()],
            vector[
                ascii::string(
                    b"28ac483b6f2b62dd58abdf0bbc3f86900d86bbdc710c704ba0b33b7f1c4b43c8::btc::BTC",
                ),
                ascii::string(
                    b"0xbd69a54e7c754a332804f325307c6627c06631dc41037239707e3242bc542e99::eth::ETH",
                ),
            ],
            vector[100, 100],
        );
        let payload = add_tokens_on_sui_message.extract_add_tokens_on_sui();
        assert!(payload.is_native() == false);
        assert!(
            payload.token_ids() == vector[treasury.token_id<BTC>(), treasury.token_id<ETH>()], 
        );
        assert!(
            payload.token_type_names() == 
                vector[
                    ascii::string(b"28ac483b6f2b62dd58abdf0bbc3f86900d86bbdc710c704ba0b33b7f1c4b43c8::btc::BTC"), 
                    ascii::string(b"0xbd69a54e7c754a332804f325307c6627c06631dc41037239707e3242bc542e99::eth::ETH"),
                ], 
        );
        assert!(payload.token_prices() == vector[100, 100]);
        assert!(
            payload == make_add_token_on_sui(
                false,
                vector[treasury.token_id<BTC>(), treasury.token_id<ETH>()],
                vector[ascii::string(b"28ac483b6f2b62dd58abdf0bbc3f86900d86bbdc710c704ba0b33b7f1c4b43c8::btc::BTC"), ascii::string(b"0xbd69a54e7c754a332804f325307c6627c06631dc41037239707e3242bc542e99::eth::ETH")],
                vector[100, 100],
            ),
        );
        // Test message serialization
        let message = serialize_message(add_tokens_on_sui_message);
        let expected_msg = hex::decode(
            b"060100000000000000010200020102024a323861633438336236663262363264643538616264663062626333663836393030643836626264633731306337303462613062333362376631633462343363383a3a6274633a3a4254434c3078626436396135346537633735346133333238303466333235333037633636323763303636333164633431303337323339373037653332343262633534326539393a3a6574683a3a4554480264000000000000006400000000000000",
        );
        assert_eq(message, expected_msg);
        assert!(add_tokens_on_sui_message == deserialize_message_test_only(message));

        destroy(treasury);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_add_tokens_on_sui_message_serialization_2() {
        let mut scenario = test_scenario::begin(@0x1);
        let ctx = test_scenario::ctx(&mut scenario);
        let treasury = treasury::mock_for_test(ctx);

        let add_tokens_on_sui_message = create_add_tokens_on_sui_message(
            chain_ids::sui_custom(),
            0, // seq_num
            false, // native_token
            vector[1, 2, 3, 4],
            vector[
                ascii::string(
                    b"9b5e13bcd0cb23ff25c07698e89d48056c745338d8c9dbd033a4172b87027073::btc::BTC",
                ),
                ascii::string(
                    b"7970d71c03573f540a7157f0d3970e117effa6ae16cefd50b45c749670b24e6a::eth::ETH",
                ),
                ascii::string(
                    b"500e429a24478405d5130222b20f8570a746b6bc22423f14b4d4e6a8ea580736::usdc::USDC",
                ),
                ascii::string(
                    b"46bfe51da1bd9511919a92eb1154149b36c0f4212121808e13e3e5857d607a9c::usdt::USDT",
                )
            ],
            vector[500_000_000, 30_000_000, 1_000, 1_000],
        );
        let payload = add_tokens_on_sui_message.extract_add_tokens_on_sui();
        assert!(
            payload == make_add_token_on_sui(
                false,
                vector[1, 2, 3, 4],
                vector[
                    ascii::string(b"9b5e13bcd0cb23ff25c07698e89d48056c745338d8c9dbd033a4172b87027073::btc::BTC"),
                    ascii::string(b"7970d71c03573f540a7157f0d3970e117effa6ae16cefd50b45c749670b24e6a::eth::ETH"),
                    ascii::string(b"500e429a24478405d5130222b20f8570a746b6bc22423f14b4d4e6a8ea580736::usdc::USDC"),
                    ascii::string(b"46bfe51da1bd9511919a92eb1154149b36c0f4212121808e13e3e5857d607a9c::usdt::USDT")
                ],
                vector[500_000_000, 30_000_000, 1_000, 1_000],
            ),
        );
        // Test message serialization
        let message = serialize_message(add_tokens_on_sui_message);
        let expected_msg = hex::decode(
            b"0601000000000000000002000401020304044a396235653133626364306362323366663235633037363938653839643438303536633734353333386438633964626430333361343137326238373032373037333a3a6274633a3a4254434a373937306437316330333537336635343061373135376630643339373065313137656666613661653136636566643530623435633734393637306232346536613a3a6574683a3a4554484c353030653432396132343437383430356435313330323232623230663835373061373436623662633232343233663134623464346536613865613538303733363a3a757364633a3a555344434c343662666535316461316264393531313931396139326562313135343134396233366330663432313231323138303865313365336535383537643630376139633a3a757364743a3a55534454040065cd1d0000000080c3c90100000000e803000000000000e803000000000000",
        );
        assert_eq(message, expected_msg);
        assert!(add_tokens_on_sui_message == deserialize_message_test_only(message));

        let mut message_bytes = b"SUI_BRIDGE_MESSAGE";
        message_bytes.append(message);

        let pubkey = sui::ecdsa_k1::secp256k1_ecrecover(
            &x"b75e64b040eef6fa510e4b9be853f0d35183de635c6456c190714f9546b163ba12583e615a2e9944ec2d21b520aebd9b14e181dcae0fcc6cdaefc0aa235b3abe00",
            &message_bytes,
            0,
        );

        assert_eq(pubkey, x"025a8c385af9a76aa506c395e240735839cb06531301f9b396e5f9ef8eeb0d8879");
        destroy(treasury);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_be_to_le_conversion() {
        let input = hex::decode(b"78563412");
        let expected = hex::decode(b"12345678");
        assert!(reverse_bytes_test(input) == expected)
    }

    #[test]
    fun test_peel_u64_be() {
        let input = hex::decode(b"0000000000003039");
        let expected = 12345u64;
        let mut bcs = bcs::new(input);
        assert!(peel_u64_be_for_testing(&mut bcs) == expected)
    }

    #[test]
    #[expected_failure(abort_code = bridge::message::ETrailingBytes)]
    fun test_bad_payload() {
        let sender_address = address::from_u256(100);
        let mut scenario = test_scenario::begin(@0x1);
        let ctx = test_scenario::ctx(&mut scenario);
        let coin = coin::mint_for_testing<USDC>(12345, ctx);
        let mut token_bridge_message = create_token_bridge_message(
            chain_ids::sui_testnet(), // source chain
            10, // seq_num
            address::to_bytes(sender_address), // sender address
            chain_ids::eth_sepolia(), // target_chain
            // Eth address is 20 bytes long
            hex::decode(b"00000000000000000000000000000000000000c8"), // target_address
            3u8, // token_type
            balance::value(coin::balance(&coin)), // amount: u64
        );
        let mut payload = token_bridge_message.payload();
        payload.push_back(0u8);
        token_bridge_message.set_payload(payload);

        token_bridge_message.extract_token_bridge_payload();

        abort 0
    }

    #[test]
    #[expected_failure(abort_code = bridge::message::ETrailingBytes)]
    fun test_bad_emergency_op() {
        let mut msg = create_emergency_op_message(
            chain_ids::sui_testnet(),
            0,
            emergency_op_pause(),
        );
        let mut payload = msg.payload();
        payload.push_back(0u8);
        msg.set_payload(payload);
        msg.extract_emergency_op_payload();
    }

    #[test]
    #[expected_failure(abort_code = bridge::message::EEmptyList)]
    fun test_bad_blocklist() {
        let blocklist_message = create_blocklist_message(
            chain_ids::sui_testnet(), 10, 0, vector[],
        );
        blocklist_message.extract_blocklist_payload();
    }

    #[test]
    #[expected_failure(abort_code = bridge::message::ETrailingBytes)]
    fun test_bad_blocklist_1() {
        let mut blocklist_message = default_blocklist_message();
        let mut payload = blocklist_message.payload();
        payload.push_back(0u8);
        blocklist_message.set_payload(payload);
        blocklist_message.extract_blocklist_payload();
    }

    #[test]
    #[expected_failure(abort_code = bridge::message::EInvalidAddressLength)]
    fun test_bad_blocklist_2() {
        let validator_pub_key1 = hex::decode(b"b14d3c4f5fbfbcfb98af2d330000d49c95b93aa7");
        // bad address
        let validator_pub_key2 = hex::decode(b"f7e93cc543d97af6632c9b8864417379dba4bf150000");
        let validator_eth_addresses = vector[validator_pub_key1, validator_pub_key2];
        create_blocklist_message(chain_ids::sui_testnet(), 10, 0, validator_eth_addresses);
    }

    #[test]
    #[expected_failure(abort_code = bridge::message::ETrailingBytes)]
    fun test_bad_bridge_limit() {
        let mut update_bridge_limit = create_update_bridge_limit_message(
            chain_ids::sui_testnet(),
            10,
            chain_ids::eth_sepolia(),
            1000000000,
        );
        let mut payload = update_bridge_limit.payload();
        payload.push_back(0u8);
        update_bridge_limit.set_payload(payload);
        update_bridge_limit.extract_update_bridge_limit();
    }

    #[test]
    #[expected_failure(abort_code = bridge::message::ETrailingBytes)]
    fun test_bad_update_price() {
        let mut asset_price_message = create_update_asset_price_message(
            2,
            chain_ids::sui_testnet(), // source chain
            10, // seq_num
            12345,
        );
        let mut payload = asset_price_message.payload();
        payload.push_back(0u8);
        asset_price_message.set_payload(payload);
        asset_price_message.extract_update_asset_price();
    }

    #[test]
    #[expected_failure(abort_code = bridge::message::ETrailingBytes)]
    fun test_bad_add_token() {
        let mut scenario = test_scenario::begin(@0x1);
        let ctx = test_scenario::ctx(&mut scenario);
        let treasury = treasury::mock_for_test(ctx);

        let mut add_token_message = create_add_tokens_on_sui_message(
            chain_ids::sui_custom(),
            1, // seq_num
            false, // native_token
            vector[treasury.token_id<BTC>(), treasury.token_id<ETH>()],
            vector[
                ascii::string(
                    b"28ac483b6f2b62dd58abdf0bbc3f86900d86bbdc710c704ba0b33b7f1c4b43c8::btc::BTC",
                ),
                ascii::string(
                    b"0xbd69a54e7c754a332804f325307c6627c06631dc41037239707e3242bc542e99::eth::ETH",
                ),
            ],
            vector[100, 100],
        );
        let mut payload = add_token_message.payload();
        payload.push_back(0u8);
        add_token_message.set_payload(payload);
        add_token_message.extract_add_tokens_on_sui();

        abort 0
    }

    #[test]
    #[expected_failure(abort_code = bridge::message::EInvalidPayloadLength)]
    fun test_bad_payload_size() {
        let sender_address = address::from_u256(100);
        let mut scenario = test_scenario::begin(sender_address);
        let ctx = test_scenario::ctx(&mut scenario);
        let coin = coin::mint_for_testing<USDC>(12345, ctx);
        let mut sender = address::to_bytes(sender_address);
        // double sender which wil make the payload different the 64 bytes
        sender.append(address::to_bytes(sender_address));
        create_token_bridge_message(
            chain_ids::sui_testnet(), // source chain
            10, // seq_num
            sender, // sender address
            chain_ids::eth_sepolia(), // target_chain
            // Eth address is 20 bytes long
            hex::decode(b"00000000000000000000000000000000000000c8"), // target_address
            3u8, // token_type
            balance::value(coin::balance(&coin)), // amount: u64
        );

        abort 0
    }

    #[test]
    #[expected_failure(abort_code = bridge::message::EMustBeTokenMessage)]
    fun test_bad_token_transfer_type() {
        let msg = create_update_asset_price_message(2, chain_ids::sui_testnet(), 10, 12345);
        msg.to_parsed_token_transfer_message();
    }

    #[test]
    fun test_voting_power() {
        let sender_address = address::from_u256(100);
        let mut scenario = test_scenario::begin(sender_address);

        let ctx = test_scenario::ctx(&mut scenario);
        let coin = coin::mint_for_testing<USDC>(12345, ctx);
        let message = default_token_bridge_message(
            sender_address,
            &coin,
            chain_ids::sui_testnet(),
            chain_ids::eth_sepolia(),
        );
        assert!(message.required_voting_power() == 3334);

        let treasury = treasury::mock_for_test(ctx);
        let message = create_add_tokens_on_sui_message(
            chain_ids::sui_custom(),
            1, // seq_num
            false, // native_token
            vector[treasury.token_id<BTC>(), treasury.token_id<ETH>()],
            vector[
                ascii::string(
                    b"28ac483b6f2b62dd58abdf0bbc3f86900d86bbdc710c704ba0b33b7f1c4b43c8::btc::BTC",
                ),
                ascii::string(
                    b"0xbd69a54e7c754a332804f325307c6627c06631dc41037239707e3242bc542e99::eth::ETH",
                ),
            ],
            vector[100, 100],
        );
        assert!(message.required_voting_power() == 5001);

        destroy(treasury);
        coin::burn_for_testing(coin);
        test_scenario::end(scenario);

        let message = create_emergency_op_message(
            chain_ids::sui_testnet(),
            10,
            emergency_op_pause(),
        );
        assert!(message.required_voting_power() == 450);
        let message = create_emergency_op_message(
            chain_ids::sui_testnet(),
            10,
            emergency_op_unpause(),
        );
        assert!(message.required_voting_power() == 5001);

        let message = default_blocklist_message();
        assert!(message.required_voting_power() == 5001);

        let message = create_update_asset_price_message(2, chain_ids::sui_testnet(), 10, 12345);
        assert!(message.required_voting_power() == 5001);

        let message = create_update_bridge_limit_message(
            chain_ids::sui_testnet(), // source chain
            10, // seq_num
            chain_ids::eth_sepolia(),
            1000000000,
        );
        assert!(message.required_voting_power() == 5001);
    }

    #[test]
    #[expected_failure(abort_code = bridge::message::EInvalidEmergencyOpType)]
    fun test_bad_voting_power_1() {
        let message = create_emergency_op_message(chain_ids::sui_testnet(), 10, 3);
        message.required_voting_power();
    }

    #[test]
    #[expected_failure(abort_code = bridge::message::EInvalidMessageType)]
    fun test_bad_voting_power_2() {
        let message = make_generic_message(
            100, // bad message type
            1, 10, chain_ids::sui_testnet(), vector[],
        );
        message.required_voting_power();
    }

    fun default_token_bridge_message<T>(
        sender: address,
        coin: &Coin<T>,
        source_chain: u8,
        target_chain: u8,
    ): BridgeMessage {
        create_token_bridge_message(
            source_chain,
            10, // seq_num
            address::to_bytes(sender),
            target_chain,
            // Eth address is 20 bytes long
            hex::decode(b"00000000000000000000000000000000000000c8"),
            3u8, // token_type
            balance::value<T>(coin::balance(coin)), // amount: u64
        )
    }

    #[test]
    #[expected_failure(abort_code = bridge::chain_ids::EInvalidBridgeRoute)]
    fun test_invalid_chain_id_1() {
        let sender_address = address::from_u256(100);
        let mut scenario = test_scenario::begin(sender_address);
        let ctx = test_scenario::ctx(&mut scenario);
        let coin = coin::mint_for_testing<USDC>(1, ctx);
        default_token_bridge_message(
            sender_address,
            &coin,
            INVALID_CHAIN,
            chain_ids::eth_sepolia(),
        );
        abort 0
    }

    #[test]
    #[expected_failure(abort_code = bridge::chain_ids::EInvalidBridgeRoute)]
    fun test_invalid_chain_id_2() {
        let sender_address = address::from_u256(100);
        let mut scenario = test_scenario::begin(sender_address);
        let ctx = test_scenario::ctx(&mut scenario);
        let coin = coin::mint_for_testing<USDC>(1, ctx);
        default_token_bridge_message(
            sender_address,
            &coin,
            chain_ids::sui_testnet(),
            INVALID_CHAIN,
        );
        abort 0
    }

    #[test]
    #[expected_failure(abort_code = bridge::chain_ids::EInvalidBridgeRoute)]
    fun test_invalid_chain_id_3() {
        create_emergency_op_message(
            INVALID_CHAIN,
            10, // seq_num
            emergency_op_pause(),
        );
        abort 0
    }

    #[test]
    #[expected_failure(abort_code = bridge::chain_ids::EInvalidBridgeRoute)]
    fun test_invalid_chain_id_4() {
        create_blocklist_message(INVALID_CHAIN, 10, 0, vector[]);
        abort 0
    }

    #[test]
    #[expected_failure(abort_code = bridge::chain_ids::EInvalidBridgeRoute)]
    fun test_invalid_chain_id_5() {
        create_update_bridge_limit_message(INVALID_CHAIN, 1, chain_ids::eth_sepolia(), 1);
        abort 0
    }

    #[test]
    #[expected_failure(abort_code = bridge::chain_ids::EInvalidBridgeRoute)]
    fun test_invalid_chain_id_6() {
        create_update_bridge_limit_message(chain_ids::sui_testnet(), 1, INVALID_CHAIN, 1);
        abort 0
    }

    #[test]
    #[expected_failure(abort_code = bridge::chain_ids::EInvalidBridgeRoute)]
    fun test_invalid_chain_id_7() {
        create_update_asset_price_message(2, INVALID_CHAIN, 1, 5);
        abort 0
    }

    #[test]
    #[expected_failure(abort_code = bridge::chain_ids::EInvalidBridgeRoute)]
    fun test_invalid_chain_id_8() {
        create_add_tokens_on_sui_message(INVALID_CHAIN, 1, false, vector[], vector[], vector[]);
        abort 0
    }

    fun default_blocklist_message(): BridgeMessage {
        let validator_pub_key1 = hex::decode(b"b14d3c4f5fbfbcfb98af2d330000d49c95b93aa7");
        let validator_pub_key2 = hex::decode(b"f7e93cc543d97af6632c9b8864417379dba4bf15");
        let validator_eth_addresses = vector[validator_pub_key1, validator_pub_key2];
        create_blocklist_message(chain_ids::sui_testnet(), 10, 0, validator_eth_addresses)
    }
}
