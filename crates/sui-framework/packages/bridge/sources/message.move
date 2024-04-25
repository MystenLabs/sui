// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module bridge::message {
    use std::ascii::{Self, String};
    use sui::bcs::{Self, BCS};

    use bridge::chain_ids;
    use bridge::message_types;
    #[test_only]
    use bridge::treasury;

    #[test_only]
    use sui::{address, balance, coin, hex, test_scenario, test_utils::{assert_eq, destroy}};
    #[test_only]
    use bridge::treasury::{BTC, ETH, USDC};

    const CURRENT_MESSAGE_VERSION: u8 = 1;
    const ECDSA_ADDRESS_LENGTH: u64 = 20;

    const ETrailingBytes: u64 = 0;
    const EInvalidAddressLength: u64 = 1;
    const EEmptyList: u64 = 2;
    const EInvalidMessageType: u64 = 3;
    const EInvalidEmergencyOpType: u64 = 4;
    const EInvalidPayloadLength: u64 = 5;

    // Emergency Op types
    const PAUSE: u8 = 0;
    const UNPAUSE: u8 = 1;

    public struct BridgeMessage has copy, drop, store {
        message_type: u8,
        message_version: u8,
        seq_num: u64,
        source_chain: u8,
        payload: vector<u8>
    }

    public struct BridgeMessageKey has copy, drop, store {
        source_chain: u8,
        message_type: u8,
        bridge_seq_num: u64
    }

    public struct TokenPayload has drop {
        sender_address: vector<u8>,
        target_chain: u8,
        target_address: vector<u8>,
        token_type: u8,
        amount: u64
    }

    public struct EmergencyOp has drop {
        op_type: u8
    }

    public struct Blocklist has drop {
        blocklist_type: u8,
        validator_eth_addresses: vector<vector<u8>>
    }

    // Update the limit for route from sending_chain to receiving_chain
    // This message is supposed to be processed by `chain` or the receiving chain
    public struct UpdateBridgeLimit has drop {
        // The receiving chain, also the chain that checks and processes this message
        receiving_chain: u8,
        // The sending chain
        sending_chain: u8,
        limit: u64
    }

    public struct UpdateAssetPrice has drop {
        token_id: u8,
        new_price: u64
    }

    public struct AddTokenOnSui has drop {
        native_token: bool,
        token_ids: vector<u8>,
        token_type_names: vector<String>,
        token_prices: vector<u64>
    }

    // Note: `bcs::peel_vec_u8` *happens* to work here because
    // `sender_address` and `target_address` are no longer than 255 bytes.
    // Therefore their length can be represented by a single byte.
    // See `create_token_bridge_message` for the actual encoding rule.
    public fun extract_token_bridge_payload(message: &BridgeMessage): TokenPayload {
        let mut bcs = bcs::new(message.payload);
        let sender_address = bcs.peel_vec_u8();
        let target_chain = bcs.peel_u8();
        let target_address = bcs.peel_vec_u8();
        let token_type = bcs.peel_u8();
        let amount = peel_u64_be(&mut bcs);

        // TODO: add test case for invalid chain id
        // TODO: replace with `chain_ids::is_valid_chain_id()`
        chain_ids::assert_valid_chain_id(target_chain);
        assert!(bcs.into_remainder_bytes().is_empty(), ETrailingBytes);

        TokenPayload {
            sender_address,
            target_chain,
            target_address,
            token_type,
            amount
        }
    }

    /// Emergency op payload is just a single byte
    public fun extract_emergency_op_payload(message: &BridgeMessage): EmergencyOp {
        assert!(message.payload.length() == 1, ETrailingBytes);
        EmergencyOp { op_type: message.payload[0] }
    }

    public fun extract_blocklist_payload(message: &BridgeMessage): Blocklist {
        // blocklist payload should consist of one byte blocklist type, and list of 33 bytes ecdsa pub keys
        let mut bcs = bcs::new(message.payload);
        let blocklist_type = bcs.peel_u8();
        let mut address_count = bcs.peel_u8();

        // TODO: add test case for 0 value
        assert!(address_count != 0, EEmptyList);

        let mut validator_eth_addresses = vector[];
        while (address_count > 0) {
            let (mut address, mut i) = (vector[], 0);
            while (i < ECDSA_ADDRESS_LENGTH) {
                address.push_back(bcs.peel_u8());
                i = i + 1;
            };
            validator_eth_addresses.push_back(address);
            address_count = address_count - 1;
        };

        assert!(bcs.into_remainder_bytes().is_empty(), ETrailingBytes);

        Blocklist {
            blocklist_type,
            validator_eth_addresses
        }
    }

    public fun extract_update_bridge_limit(message: &BridgeMessage): UpdateBridgeLimit {
        let mut bcs = bcs::new(message.payload);
        let sending_chain = bcs.peel_u8();
        let limit = peel_u64_be(&mut bcs);

        // TODO: add test case for invalid chain id
        chain_ids::assert_valid_chain_id(sending_chain);
        assert!(bcs.into_remainder_bytes().is_empty(), ETrailingBytes);

        UpdateBridgeLimit {
            receiving_chain: message.source_chain,
            sending_chain,
            limit
        }
    }

    public fun extract_update_asset_price(message: &BridgeMessage): UpdateAssetPrice {
        let mut bcs = bcs::new(message.payload);
        let token_id = bcs.peel_u8();
        let new_price = peel_u64_be(&mut bcs);

        assert!(bcs.into_remainder_bytes().is_empty(), ETrailingBytes);

        UpdateAssetPrice {
            token_id,
            new_price
        }
    }

    public fun extract_add_tokens_on_sui(message: &BridgeMessage): AddTokenOnSui {
        let mut bcs = bcs::new(message.payload);
        let native_token = bcs.peel_bool();
        let token_ids = bcs.peel_vec_u8();
        let token_type_names_bytes = bcs.peel_vec_vec_u8();
        let token_prices = bcs.peel_vec_u64();

        let mut n = 0;
        let mut token_type_names = vector[];
        while (n < token_type_names_bytes.length()){
            token_type_names.push_back(ascii::string(*token_type_names_bytes.borrow(n)));
            n = n + 1;
        };
        assert!(bcs.into_remainder_bytes().is_empty(), ETrailingBytes);
        AddTokenOnSui {
            native_token,
            token_ids,
            token_type_names,
            token_prices
        }
    }

    public fun serialize_message(message: BridgeMessage): vector<u8> {
        let BridgeMessage {
            message_type,
            message_version,
            seq_num,
            source_chain,
            payload
        } = message;

        let mut message = vector[
            message_type,
            message_version,
        ];

        // bcs serializes u64 as 8 bytes
        message.append(reverse_bytes(bcs::to_bytes(&seq_num)));
        message.push_back(source_chain);
        message.append(payload);
        message
    }

    /// Token Transfer Message Format:
    /// [message_type: u8]
    /// [version:u8]
    /// [nonce:u64]
    /// [source_chain: u8]
    /// [sender_address_length:u8]
    /// [sender_address: byte[]]
    /// [target_chain:u8]
    /// [target_address_length:u8]
    /// [target_address: byte[]]
    /// [token_type:u8]
    /// [amount:u64]
    public fun create_token_bridge_message(
        source_chain: u8,
        seq_num: u64,
        sender_address: vector<u8>,
        target_chain: u8,
        target_address: vector<u8>,
        token_type: u8,
        amount: u64
    ): BridgeMessage {
        // TODO: add test case for invalid chain id
        // TODO: add test case for invalid chain id
        // TODO: replace with `chain_ids::is_valid_chain_id()`
        chain_ids::assert_valid_chain_id(source_chain);
        chain_ids::assert_valid_chain_id(target_chain);

        let mut payload = vector[];

        // sender address should be less than 255 bytes so can fit into u8
        payload.push_back((vector::length(&sender_address) as u8));
        payload.append(sender_address);
        payload.push_back(target_chain);
        // target address should be less than 255 bytes so can fit into u8
        payload.push_back((vector::length(&target_address) as u8));
        payload.append(target_address);
        payload.push_back(token_type);
        // bcs serialzies u64 as 8 bytes
        payload.append(reverse_bytes(bcs::to_bytes(&amount)));

        assert!(vector::length(&payload) == 64, EInvalidPayloadLength);

        BridgeMessage {
            message_type: message_types::token(),
            message_version: CURRENT_MESSAGE_VERSION,
            seq_num,
            source_chain,
            payload,
        }
    }

    /// Emergency Op Message Format:
    /// [message_type: u8]
    /// [version:u8]
    /// [nonce:u64]
    /// [chain_id: u8]
    /// [op_type: u8]
    public fun create_emergency_op_message(
        source_chain: u8,
        seq_num: u64,
        op_type: u8,
    ): BridgeMessage {
        // TODO: add test case for invalid chain id
        // TODO: replace with `chain_ids::is_valid_chain_id()`
        chain_ids::assert_valid_chain_id(source_chain);

        BridgeMessage {
            message_type: message_types::emergency_op(),
            message_version: CURRENT_MESSAGE_VERSION,
            seq_num,
            source_chain,
            payload: vector[op_type],
        }
    }

    /// Blocklist Message Format:
    /// [message_type: u8]
    /// [version:u8]
    /// [nonce:u64]
    /// [chain_id: u8]
    /// [blocklist_type: u8]
    /// [validator_length: u8]
    /// [validator_ecdsa_addresses: byte[][]]
    public fun create_blocklist_message(
        source_chain: u8,
        seq_num: u64,
        // 0: block, 1: unblock
        blocklist_type: u8,
        validator_ecdsa_addresses: vector<vector<u8>>,
    ): BridgeMessage {
        // TODO: add test case for invalid chain id
        // TODO: replace with `chain_ids::is_valid_chain_id()`
        chain_ids::assert_valid_chain_id(source_chain);

        let address_length = validator_ecdsa_addresses.length();
        let mut payload = vector[blocklist_type, (address_length as u8)];
        let mut i = 0;

        while (i < address_length) {
            let address = validator_ecdsa_addresses[i];
            assert!(address.length() == ECDSA_ADDRESS_LENGTH, EInvalidAddressLength);
            payload.append(address);

            i = i + 1;
        };

        BridgeMessage {
            message_type: message_types::committee_blocklist(),
            message_version: CURRENT_MESSAGE_VERSION,
            seq_num,
            source_chain,
            payload,
        }
    }

    /// Update bridge limit Message Format:
    /// [message_type: u8]
    /// [version:u8]
    /// [nonce:u64]
    /// [receiving_chain_id: u8]
    /// [sending_chain_id: u8]
    /// [new_limit: u64]
    public fun create_update_bridge_limit_message(
        receiving_chain: u8,
        seq_num: u64,
        sending_chain: u8,
        new_limit: u64,
    ): BridgeMessage {
        // TODO: add test case for invalid chain id
        // TODO: add test case for invalid chain id
        // TODO: replace with `chain_ids::is_valid_chain_id()`
        chain_ids::assert_valid_chain_id(receiving_chain);
        chain_ids::assert_valid_chain_id(sending_chain);

        let mut payload = vector[sending_chain];
        payload.append(reverse_bytes(bcs::to_bytes(&new_limit)));

        BridgeMessage {
            message_type: message_types::update_bridge_limit(),
            message_version: CURRENT_MESSAGE_VERSION,
            seq_num,
            source_chain: receiving_chain,
            payload,
        }
    }

    /// Update asset price message
    /// [message_type: u8]
    /// [version:u8]
    /// [nonce:u64]
    /// [chain_id: u8]
    /// [token_id: u8]
    /// [new_price:u64]
    public fun create_update_asset_price_message(
        token_id: u8,
        source_chain: u8,
        seq_num: u64,
        new_price: u64,
    ): BridgeMessage {
        // TODO: add test case for invalid chain id
        // TODO: replace with `chain_ids::is_valid_chain_id()`
        chain_ids::assert_valid_chain_id(source_chain);

        let mut payload = vector[token_id];
        payload.append(reverse_bytes(bcs::to_bytes(&new_price)));
        BridgeMessage {
            message_type: message_types::update_asset_price(),
            message_version: CURRENT_MESSAGE_VERSION,
            seq_num,
            source_chain,
            payload,
        }
    }

    /// Update Sui token message
    /// [message_type:u8]
    /// [version:u8]
    /// [nonce:u64]
    /// [chain_id: u8]
    /// [native_token:bool]
    /// [token_ids:vector<u8>]
    /// [token_type_name:vector<String>]
    /// [token_prices:vector<u64>]
    public fun create_add_tokens_on_sui_message(
        source_chain: u8,
        seq_num: u64,
        native_token: bool,
        token_ids: vector<u8>,
        type_names: vector<String>,
        token_prices: vector<u64>,
    ): BridgeMessage {
        chain_ids::assert_valid_chain_id(source_chain);
        let mut payload = bcs::to_bytes(&native_token);
        payload.append(bcs::to_bytes(&token_ids));
        payload.append(bcs::to_bytes(&type_names));
        payload.append(bcs::to_bytes(&token_prices));
        BridgeMessage {
            message_type: message_types::add_tokens_on_sui(),
            message_version: CURRENT_MESSAGE_VERSION,
            seq_num,
            source_chain,
            payload,
        }
    }

    public fun create_key(source_chain: u8, message_type: u8, bridge_seq_num: u64): BridgeMessageKey {
        BridgeMessageKey { source_chain, message_type, bridge_seq_num }
    }

    public fun key(self: &BridgeMessage): BridgeMessageKey {
        create_key(self.source_chain, self.message_type, self.seq_num)
    }

    // BridgeMessage getters
    public fun message_version(self: &BridgeMessage): u8 {
        self.message_version
    }

    public fun message_type(self: &BridgeMessage): u8 {
        self.message_type
    }

    public fun seq_num(self: &BridgeMessage): u64 {
        self.seq_num
    }

    public fun source_chain(self: &BridgeMessage): u8 {
        self.source_chain
    }

    public fun token_target_chain(self: &TokenPayload): u8 {
        self.target_chain
    }

    public fun token_target_address(self: &TokenPayload): vector<u8> {
        self.target_address
    }

    public fun token_type(self: &TokenPayload): u8 {
        self.token_type
    }

    public fun token_amount(self: &TokenPayload): u64 {
        self.amount
    }

    // EmergencyOpPayload getters
    public fun emergency_op_type(self: &EmergencyOp): u8 {
        self.op_type
    }

    public fun blocklist_type(self: &Blocklist): u8 {
        self.blocklist_type
    }

    public fun blocklist_validator_addresses(self: &Blocklist): &vector<vector<u8>> {
        &self.validator_eth_addresses
    }

    public fun update_bridge_limit_payload_sending_chain(self: &UpdateBridgeLimit): u8 {
        self.sending_chain
    }

    public fun update_bridge_limit_payload_receiving_chain(self: &UpdateBridgeLimit): u8 {
        self.receiving_chain
    }

    public fun update_bridge_limit_payload_limit(self: &UpdateBridgeLimit): u64 {
        self.limit
    }

    public fun update_asset_price_payload_token_id(self: &UpdateAssetPrice): u8 {
        self.token_id
    }

    public fun update_asset_price_payload_new_price(self: &UpdateAssetPrice): u64 {
        self.new_price
    }

    public fun is_native(self: &AddTokenOnSui): bool {
        self.native_token
    }

    public fun token_ids(self: &AddTokenOnSui): vector<u8> {
        self.token_ids
    }

    public fun token_type_names(self: &AddTokenOnSui): vector<String> {
        self.token_type_names
    }

    public fun token_prices(self: &AddTokenOnSui): vector<u64> {
        self.token_prices
    }

    public fun emergency_op_pause(): u8 {
        PAUSE
    }

    public fun emergency_op_unpause(): u8 {
        UNPAUSE
    }

    /// Return the required signature threshold for the message, values are voting power in the scale of 10000
    public fun required_voting_power(self: &BridgeMessage): u64 {
        let message_type = message_type(self);

        if (message_type == message_types::token()) {
            3334
        } else if (message_type == message_types::emergency_op()) {
            let payload = extract_emergency_op_payload(self);
            if (payload.op_type == PAUSE) {
                450
            } else if (payload.op_type == UNPAUSE) {
                5001
            } else {
                abort EInvalidEmergencyOpType
            }
        } else if (message_type == message_types::committee_blocklist()) {
            5001
        } else if (message_type == message_types::update_asset_price()) {
            5001
        } else if (message_type == message_types::update_bridge_limit()) {
            5001
        } else if (message_type == message_types::add_tokens_on_sui()) {
            5001
        } else {
            abort EInvalidMessageType
        }
    }

    fun reverse_bytes(mut bytes: vector<u8>): vector<u8> {
        vector::reverse(&mut bytes);
        bytes
    }

    fun peel_u64_be(bcs: &mut BCS): u64 {
        let (mut value, mut i) = (0u64, 64u8);
        while (i > 0) {
            i = i - 8;
            let byte = (bcs::peel_u8(bcs) as u64);
            value = value + (byte << i);
        };
        value
    }

    #[test_only]
    public fun deserialize_message_test_only(message: vector<u8>): BridgeMessage {
        let mut bcs = bcs::new(message);
        let message_type = bcs::peel_u8(&mut bcs);
        let message_version = bcs::peel_u8(&mut bcs);
        let seq_num = peel_u64_be(&mut bcs);
        let source_chain = bcs::peel_u8(&mut bcs);
        let payload = bcs::into_remainder_bytes(bcs);
        BridgeMessage {
            message_type,
            message_version,
            seq_num,
            source_chain,
            payload,
        }
    }

    #[test]
    fun test_message_serialization_sui_to_eth() {
        let sender_address = address::from_u256(100);
        let mut scenario = test_scenario::begin(sender_address);
        let ctx = test_scenario::ctx(&mut scenario);

        let coin = coin::mint_for_testing<USDC>(12345, ctx);

        let token_bridge_message = create_token_bridge_message(
            chain_ids::sui_testnet(), // source chain
            10, // seq_num
            address::to_bytes(sender_address), // sender address
            chain_ids::eth_sepolia(), // target_chain
            // Eth address is 20 bytes long
            hex::decode(b"00000000000000000000000000000000000000c8"), // target_address
            3u8, // token_type
            balance::value(coin::balance(&coin)) // amount: u64
        );

        // Test payload extraction
        let token_payload = TokenPayload {
            sender_address: address::to_bytes(sender_address),
            target_chain: chain_ids::eth_sepolia(),
            target_address: hex::decode(b"00000000000000000000000000000000000000c8"),
            token_type: 3u8,
            amount: balance::value(coin::balance(&coin))
        };
        assert!(extract_token_bridge_payload(&token_bridge_message) == token_payload, 0);

        // Test message serialization
        let message = serialize_message(token_bridge_message);
        let expected_msg = hex::decode(
            b"0001000000000000000a012000000000000000000000000000000000000000000000000000000000000000640b1400000000000000000000000000000000000000c8030000000000003039",
        );

        assert!(message == expected_msg, 0);
        assert!(token_bridge_message == deserialize_message_test_only(message), 0);

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
            balance::value(coin::balance(&coin)) // amount: u64
        );

        // Test payload extraction
        let token_payload = TokenPayload {
            sender_address: hex::decode(b"00000000000000000000000000000000000000c8"),
            target_chain: chain_ids::sui_testnet(),
            target_address: address::to_bytes(address_1),
            token_type: 3u8,
            amount: balance::value(coin::balance(&coin))
        };
        assert!(extract_token_bridge_payload(&token_bridge_message) == token_payload, 0);


        // Test message serialization
        let message = serialize_message(token_bridge_message);
        let expected_msg = hex::decode(
            b"0001000000000000000a0b1400000000000000000000000000000000000000c801200000000000000000000000000000000000000000000000000000000000000064030000000000003039",
        );
        assert!(message == expected_msg, 0);
        assert!(token_bridge_message == deserialize_message_test_only(message), 0);

        coin::burn_for_testing(coin);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_emergency_op_message_serialization() {
        let emergency_op_message = create_emergency_op_message(
            chain_ids::sui_testnet(), // source chain
            10, // seq_num
            0,
        );

        // Test message serialization
        let message = serialize_message(emergency_op_message);
        let expected_msg = hex::decode(
            b"0201000000000000000a0100",
        );

        assert!(message == expected_msg, 0);
        assert!(emergency_op_message == deserialize_message_test_only(message), 0);
    }

    // Do not change/remove this test, it uses move bytes generated by Rust
    #[test]
    fun test_emergency_op_message_serialization_regression() {
        let emergency_op_message = create_emergency_op_message(
            chain_ids::sui_custom(),
            55, // seq_num
            0, // pause
        );

        // Test message serialization
        let message = serialize_message(emergency_op_message);
        let expected_msg = hex::decode(
            b"020100000000000000370200",
        );

        assert_eq(expected_msg, message);
        assert!(emergency_op_message == deserialize_message_test_only(message), 0);
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
            validator_eth_addresses
        );
        // Test message serialization
        let message = serialize_message(blocklist_message);

        let expected_msg = hex::decode(
            b"0101000000000000000a010002b14d3c4f5fbfbcfb98af2d330000d49c95b93aa7f7e93cc543d97af6632c9b8864417379dba4bf15",
        );

        assert!(message == expected_msg, 0);
        assert!(blocklist_message == deserialize_message_test_only(message), 0);

        let blocklist = extract_blocklist_payload(&blocklist_message);
        assert!(blocklist.validator_eth_addresses == validator_eth_addresses, 0)
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
            validator_eth_addresses
        );
        // Test message serialization
        let message = serialize_message(blocklist_message);

        let expected_msg = hex::decode(
            b"0101000000000000008102000168b43fd906c0b8f024a18c56e06744f7c6157c65",
        );

        assert_eq(expected_msg, message);
        assert!(blocklist_message == deserialize_message_test_only(message), 0);

        let blocklist = extract_blocklist_payload(&blocklist_message);
        assert!(blocklist.validator_eth_addresses == validator_eth_addresses, 0);

        // Test 2
        let validator_eth_addresses = vector[validator_eth_addr_1, validator_eth_addr_2];
        let blocklist_message = create_blocklist_message(
            chain_ids::sui_custom(), // source chain
            68, // seq_num
            1, // unblocklist
            validator_eth_addresses
        );
        // Test message serialization
        let message = serialize_message(blocklist_message);

        let expected_msg = hex::decode(
            b"0101000000000000004402010268b43fd906c0b8f024a18c56e06744f7c6157c65acaef39832cb995c4e049437a3e2ec6a7bad1ab5",
        );

        assert_eq(expected_msg, message);
        assert!(blocklist_message == deserialize_message_test_only(message), 0);

        let blocklist = extract_blocklist_payload(&blocklist_message);
        assert!(blocklist.validator_eth_addresses == validator_eth_addresses, 0)
    }

    #[test]
    fun test_update_bridge_limit_message_serialization() {
        let update_bridge_limit = create_update_bridge_limit_message(
            chain_ids::sui_testnet(), // source chain
            10, // seq_num
            chain_ids::eth_sepolia(),
            1000000000
        );

        // Test message serialization
        let message = serialize_message(update_bridge_limit);
        let expected_msg = hex::decode(
            b"0301000000000000000a010b000000003b9aca00",
        );

        assert!(message == expected_msg, 0);
        assert!(update_bridge_limit == deserialize_message_test_only(message), 0);

        let bridge_limit = extract_update_bridge_limit(&update_bridge_limit);
        assert!(bridge_limit.receiving_chain == chain_ids::sui_testnet(), 0);
        assert!(bridge_limit.sending_chain == chain_ids::eth_sepolia(), 0);
        assert!(bridge_limit.limit == 1000000000, 0);
    }

    // Do not change/remove this test, it uses move bytes generated by Rust
    #[test]
    fun test_update_bridge_limit_message_serialization_regression() {
        let update_bridge_limit = create_update_bridge_limit_message(
            chain_ids::sui_custom(), // source chain
            15, // seq_num
            chain_ids::eth_custom(),
            10_000_000_000 // 1M USD
        );

        // Test message serialization
        let message = serialize_message(update_bridge_limit);
        let expected_msg = hex::decode(
            b"0301000000000000000f020c00000002540be400",
        );

        assert_eq(message, expected_msg);
        assert!(update_bridge_limit == deserialize_message_test_only(message), 0);

        let bridge_limit = extract_update_bridge_limit(&update_bridge_limit);
        assert!(bridge_limit.receiving_chain == chain_ids::sui_custom(), 0);
        assert!(bridge_limit.sending_chain == chain_ids::eth_custom(), 0);
        assert!(bridge_limit.limit == 10_000_000_000, 0);
    }

    #[test]
    fun test_update_asset_price_message_serialization() {
        let asset_price_message = create_update_asset_price_message(
            2,
            chain_ids::sui_testnet(), // source chain
            10, // seq_num
            12345
        );

        // Test message serialization
        let message = serialize_message(asset_price_message);
        let expected_msg = hex::decode(
            b"0401000000000000000a01020000000000003039",
        );
        assert!(message == expected_msg, 0);
        assert!(asset_price_message == deserialize_message_test_only(message), 0);

        let asset_price = extract_update_asset_price(&asset_price_message);

        let mut scenario = test_scenario::begin(@0x1);
        let ctx = test_scenario::ctx(&mut scenario);
        let treasury = treasury::mock_for_test(ctx);

        assert!(asset_price.token_id == treasury::token_id<ETH>(&treasury), 0);
        assert!(asset_price.new_price == 12345, 0);

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
            1_000_000_000 // $100k USD
        );

        // Test message serialization
        let message = serialize_message(asset_price_message);
        let expected_msg = hex::decode(
            b"0401000000000000010a0201000000003b9aca00",
        );
        assert_eq(expected_msg, message);
        assert!(asset_price_message == deserialize_message_test_only(message), 0);

        let asset_price = extract_update_asset_price(&asset_price_message);

        assert!(asset_price.token_id == treasury::token_id<BTC>(&treasury), 0);
        assert!(asset_price.new_price == 1_000_000_000, 0);

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
            vector[ascii::string(b"28ac483b6f2b62dd58abdf0bbc3f86900d86bbdc710c704ba0b33b7f1c4b43c8::btc::BTC"), ascii::string(b"0xbd69a54e7c754a332804f325307c6627c06631dc41037239707e3242bc542e99::eth::ETH")],
            vector[100, 100]
        );
        let payload = extract_add_tokens_on_sui(&add_tokens_on_sui_message);
        assert!(payload == AddTokenOnSui {
            native_token: false,
            token_ids: vector[treasury.token_id<BTC>(), treasury.token_id<ETH>()],
            token_type_names: vector[ascii::string(b"28ac483b6f2b62dd58abdf0bbc3f86900d86bbdc710c704ba0b33b7f1c4b43c8::btc::BTC"), ascii::string(b"0xbd69a54e7c754a332804f325307c6627c06631dc41037239707e3242bc542e99::eth::ETH")],
            token_prices: vector[100, 100],
        }, 0);
        // Test message serialization
        let message = serialize_message(add_tokens_on_sui_message);
        let expected_msg = hex::decode(
            b"060100000000000000010200020102024a323861633438336236663262363264643538616264663062626333663836393030643836626264633731306337303462613062333362376631633462343363383a3a6274633a3a4254434c3078626436396135346537633735346133333238303466333235333037633636323763303636333164633431303337323339373037653332343262633534326539393a3a6574683a3a4554480264000000000000006400000000000000",
        );
        assert_eq(message, expected_msg);
        assert!(add_tokens_on_sui_message == deserialize_message_test_only(message), 0);

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
                ascii::string(b"9b5e13bcd0cb23ff25c07698e89d48056c745338d8c9dbd033a4172b87027073::btc::BTC"),
                ascii::string(b"7970d71c03573f540a7157f0d3970e117effa6ae16cefd50b45c749670b24e6a::eth::ETH"),
                ascii::string(b"500e429a24478405d5130222b20f8570a746b6bc22423f14b4d4e6a8ea580736::usdc::USDC"),
                ascii::string(b"46bfe51da1bd9511919a92eb1154149b36c0f4212121808e13e3e5857d607a9c::usdt::USDT")
            ],
            vector[500_000_000, 30_000_000, 1_000, 1_000]
        );
        let payload = extract_add_tokens_on_sui(&add_tokens_on_sui_message);
        assert!(payload == AddTokenOnSui {
            native_token: false,
            token_ids: vector[1, 2, 3, 4],
            token_type_names: vector[
                ascii::string(b"9b5e13bcd0cb23ff25c07698e89d48056c745338d8c9dbd033a4172b87027073::btc::BTC"),
                ascii::string(b"7970d71c03573f540a7157f0d3970e117effa6ae16cefd50b45c749670b24e6a::eth::ETH"),
                ascii::string(b"500e429a24478405d5130222b20f8570a746b6bc22423f14b4d4e6a8ea580736::usdc::USDC"),
                ascii::string(b"46bfe51da1bd9511919a92eb1154149b36c0f4212121808e13e3e5857d607a9c::usdt::USDT")
            ],
            token_prices: vector[500_000_000, 30_000_000, 1_000, 1_000]
        }, 0);
        // Test message serialization
        let message = serialize_message(add_tokens_on_sui_message);
        let expected_msg = hex::decode(
            b"0601000000000000000002000401020304044a396235653133626364306362323366663235633037363938653839643438303536633734353333386438633964626430333361343137326238373032373037333a3a6274633a3a4254434a373937306437316330333537336635343061373135376630643339373065313137656666613661653136636566643530623435633734393637306232346536613a3a6574683a3a4554484c353030653432396132343437383430356435313330323232623230663835373061373436623662633232343233663134623464346536613865613538303733363a3a757364633a3a555344434c343662666535316461316264393531313931396139326562313135343134396233366330663432313231323138303865313365336535383537643630376139633a3a757364743a3a55534454040065cd1d0000000080c3c90100000000e803000000000000e803000000000000",
        );
        assert_eq(message, expected_msg);
        assert!(add_tokens_on_sui_message == deserialize_message_test_only(message), 0);

        let mut message_bytes = b"SUI_BRIDGE_MESSAGE";
        message_bytes.append(message);

        let pubkey = sui::ecdsa_k1::secp256k1_ecrecover(
            &x"b75e64b040eef6fa510e4b9be853f0d35183de635c6456c190714f9546b163ba12583e615a2e9944ec2d21b520aebd9b14e181dcae0fcc6cdaefc0aa235b3abe00"
            , &message_bytes, 0);

        assert_eq(pubkey, x"025a8c385af9a76aa506c395e240735839cb06531301f9b396e5f9ef8eeb0d8879");
        destroy(treasury);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_be_to_le_conversion() {
        let input = hex::decode(b"78563412");
        let expected = hex::decode(b"12345678");
        assert!(reverse_bytes(input) == expected, 0)
    }

    #[test]
    fun test_peel_u64_be() {
        let input = hex::decode(b"0000000000003039");
        let expected = 12345u64;
        let mut bcs = bcs::new(input);
        assert!(peel_u64_be(&mut bcs) == expected, 0)
    }
}
