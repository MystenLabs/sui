// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module bridge::message {
    use std::vector;

    use sui::bcs;
    use sui::bcs::BCS;

    use bridge::chain_ids;
    use bridge::message_types;
    use bridge::treasury;

    #[test_only]
    use sui::address;
    #[test_only]
    use sui::balance;
    #[test_only]
    use sui::coin;
    #[test_only]
    use sui::hex;
    #[test_only]
    use sui::test_scenario;
    #[test_only]
    use sui::test_utils::assert_eq;
    #[test_only]
    use bridge::btc::BTC;
    #[test_only]
    use bridge::eth::ETH;
    #[test_only]
    use bridge::treasury::token_id;
    #[test_only]
    use bridge::usdc::USDC;

    const CURRENT_MESSAGE_VERSION: u8 = 1;
    const ECDSA_ADDRESS_LENGTH: u64 = 20;

    const ETrailingBytes: u64 = 0;
    const EInvalidAddressLength: u64 = 1;
    const EEmptyList: u64 = 2;
    const EInvalidMessageType: u64 = 3;
    const EInvalidEmergencyOpType: u64 = 4;

    // Emergency Op types
    const PAUSE: u8 = 0;
    const UNPAUSE: u8 = 1;

    struct BridgeMessage has copy, drop, store {
        message_type: u8,
        message_version: u8,
        seq_num: u64,
        source_chain: u8,
        payload: vector<u8>
    }

    struct BridgeMessageKey has copy, drop, store {
        source_chain: u8,
        message_type: u8,
        bridge_seq_num: u64
    }

    struct TokenPayload has drop {
        sender_address: vector<u8>,
        target_chain: u8,
        target_address: vector<u8>,
        token_type: u8,
        amount: u64
    }

    struct EmergencyOp has drop {
        op_type: u8
    }

    struct Blocklist has drop {
        blocklist_type: u8,
        validator_eth_addresses: vector<vector<u8>>
    }

    // Update the limit for route from sending_chain to receiving_chain
    // This message is supposed to be processed by `chain` or the receiving chain
    struct UpdateBridgeLimit has drop {
        // The receiving chain, also the chain that checks and processes this message
        receiving_chain: u8,
        // The sending chain
        sending_chain: u8,
        limit: u64
    }

    struct UpdateAssetPrice has drop {
        token_id: u8,
        new_price: u64
    }

    // Note: `bcs::peel_vec_u8` *happens* to work here because
    // `sender_address` and `target_address` are no longer than 255 bytes.
    // Therefore their length can be represented by a single byte.
    // See `create_token_bridge_message` for the actual encoding rule.
    public fun extract_token_bridge_payload(message: &BridgeMessage): TokenPayload {
        let bcs = bcs::new(message.payload);
        let sender_address = bcs::peel_vec_u8(&mut bcs);
        let target_chain = bcs::peel_u8(&mut bcs);
        // TODO: add test case for invalid chain id
        chain_ids::assert_valid_chain_id(target_chain);
        let target_address = bcs::peel_vec_u8(&mut bcs);
        let token_type = bcs::peel_u8(&mut bcs);
        let amount = peel_u64_be(&mut bcs);
        assert!(vector::is_empty(&bcs::into_remainder_bytes(bcs)), ETrailingBytes);
        TokenPayload {
            sender_address,
            target_chain,
            target_address,
            token_type,
            amount
        }
    }

    public fun extract_emergency_op_payload(message: &BridgeMessage): EmergencyOp {
        // emergency op payload is just a single byte
        assert!(vector::length(&message.payload) == 1, ETrailingBytes);
        EmergencyOp {
            op_type: *vector::borrow(&message.payload, 0)
        }
    }

    public fun extract_blocklist_payload(message: &BridgeMessage): Blocklist {
        // blocklist payload should consist of one byte blocklist type, and list of 33 bytes ecdsa pub keys
        let bcs = bcs::new(message.payload);
        let blocklist_type = bcs::peel_u8(&mut bcs);
        let address_count = bcs::peel_u8(&mut bcs);
        // TODO: add test case for 0 value
        assert!(address_count != 0, EEmptyList);
        let validator_eth_addresses = vector[];
        while (address_count > 0) {
            let (address, i) = (vector[], 0);
            while (i < ECDSA_ADDRESS_LENGTH) {
                vector::push_back(&mut address, bcs::peel_u8(&mut bcs));
                i = i + 1;
            };
            vector::push_back(&mut validator_eth_addresses, address);
            address_count = address_count - 1;
        };
        assert!(vector::is_empty(&bcs::into_remainder_bytes(bcs)), ETrailingBytes);
        Blocklist {
            blocklist_type,
            validator_eth_addresses
        }
    }

    public fun extract_update_bridge_limit(message: &BridgeMessage): UpdateBridgeLimit {
        let bcs = bcs::new(message.payload);
        let sending_chain = bcs::peel_u8(&mut bcs);
        // TODO: add test case for invalid chain id
        chain_ids::assert_valid_chain_id(sending_chain);
        let limit = peel_u64_be(&mut bcs);
        assert!(vector::is_empty(&bcs::into_remainder_bytes(bcs)), ETrailingBytes);
        UpdateBridgeLimit {
            receiving_chain: message.source_chain,
            sending_chain,
            limit
        }
    }

    public fun extract_update_asset_price(message: &BridgeMessage): UpdateAssetPrice {
        let bcs = bcs::new(message.payload);
        let token_id = bcs::peel_u8(&mut bcs);
        let new_price = peel_u64_be(&mut bcs);
        assert!(vector::is_empty(&bcs::into_remainder_bytes(bcs)), ETrailingBytes);
        UpdateAssetPrice {
            token_id,
            new_price
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

        let message = vector[];
        vector::push_back(&mut message, message_type);
        vector::push_back(&mut message, message_version);
        // bcs serializes u64 as 8 bytes
        vector::append(&mut message, reverse_bytes(bcs::to_bytes(&seq_num)));
        vector::push_back(&mut message, source_chain);
        vector::append(&mut message, payload);
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
        chain_ids::assert_valid_chain_id(source_chain);
        // TODO: add test case for invalid chain id
        chain_ids::assert_valid_chain_id(target_chain);
        let payload = vector[];
        // sender address should be less than 255 bytes so can fit into u8
        vector::push_back(&mut payload, (vector::length(&sender_address) as u8));
        vector::append(&mut payload, sender_address);
        vector::push_back(&mut payload, target_chain);
        // target address should be less than 255 bytes so can fit into u8
        vector::push_back(&mut payload, (vector::length(&target_address) as u8));
        vector::append(&mut payload, target_address);
        vector::push_back(&mut payload, token_type);
        // bcs serialzies u64 as 8 bytes
        vector::append(&mut payload, reverse_bytes(bcs::to_bytes(&amount)));

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
        chain_ids::assert_valid_chain_id(source_chain);
        let address_length = (vector::length(&validator_ecdsa_addresses) as u8);
        let payload = vector[blocklist_type, address_length];
        let i = 0;

        while (i < address_length) {
            let address = vector::borrow(&validator_ecdsa_addresses, (i as u64));
            assert!(vector::length(address) == ECDSA_ADDRESS_LENGTH, EInvalidAddressLength);
            vector::append(&mut payload, *address);
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
        chain_ids::assert_valid_chain_id(receiving_chain);
        // TODO: add test case for invalid chain id
        chain_ids::assert_valid_chain_id(sending_chain);
        let payload = vector[sending_chain];
        vector::append(&mut payload, reverse_bytes(bcs::to_bytes(&new_limit)));
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
    public fun create_update_asset_price_message<T>(
        source_chain: u8,
        seq_num: u64,
        new_price: u64,
    ): BridgeMessage {
        // TODO: add test case for invalid chain id
        chain_ids::assert_valid_chain_id(source_chain);
        let payload = vector[treasury::token_id<T>()];
        vector::append(&mut payload, reverse_bytes(bcs::to_bytes(&new_price)));
        BridgeMessage {
            message_type: message_types::update_asset_price(),
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
        } else {
            abort EInvalidMessageType
        }
    }

    fun reverse_bytes(bytes: vector<u8>): vector<u8> {
        vector::reverse(&mut bytes);
        bytes
    }

    fun peel_u64_be(bcs: &mut BCS): u64 {
        let (value, i) = (0u64, 64u8);
        while (i > 0) {
            i = i - 8;
            let byte = (bcs::peel_u8(bcs) as u64);
            value = value + (byte << i);
        };
        value
    }

    #[test_only]
    public fun deserialize_message_test_only(message: vector<u8>): BridgeMessage {
        let bcs = bcs::new(message);
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
        let scenario = test_scenario::begin(sender_address);
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
        let scenario = test_scenario::begin(address_1);
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
            chain_ids::sui_local_test(),
            55, // seq_num
            0, // pause
        );

        // Test message serialization
        let message = serialize_message(emergency_op_message);
        let expected_msg = hex::decode(
            b"020100000000000000370300",
        );

        assert!(message == expected_msg, 0);
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
            chain_ids::sui_local_test(), // source chain
            129, // seq_num
            0, // blocklist
            validator_eth_addresses
        );
        // Test message serialization
        let message = serialize_message(blocklist_message);

        let expected_msg = hex::decode(
            b"0101000000000000008103000168b43fd906c0b8f024a18c56e06744f7c6157c65",
        );

        assert!(message == expected_msg, 0);
        assert!(blocklist_message == deserialize_message_test_only(message), 0);

        let blocklist = extract_blocklist_payload(&blocklist_message);
        assert!(blocklist.validator_eth_addresses == validator_eth_addresses, 0);

        // Test 2
        let validator_eth_addresses = vector[validator_eth_addr_1, validator_eth_addr_2];
        let blocklist_message = create_blocklist_message(
            chain_ids::sui_devnet(), // source chain
            68, // seq_num
            1, // unblocklist
            validator_eth_addresses
        );
        // Test message serialization
        let message = serialize_message(blocklist_message);

        let expected_msg = hex::decode(
            b"0101000000000000004402010268b43fd906c0b8f024a18c56e06744f7c6157c65acaef39832cb995c4e049437a3e2ec6a7bad1ab5",
        );

        assert!(message == expected_msg, 0);
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
            chain_ids::sui_local_test(), // source chain
            15, // seq_num
            chain_ids::eth_local_test(),
            10_000_000_000 // 1M USD
        );

        // Test message serialization
        let message = serialize_message(update_bridge_limit);
        let expected_msg = hex::decode(
            b"0301000000000000000f030c00000002540be400",
        );

        assert_eq(message, expected_msg);
        assert!(update_bridge_limit == deserialize_message_test_only(message), 0);

        let bridge_limit = extract_update_bridge_limit(&update_bridge_limit);
        assert!(bridge_limit.receiving_chain == chain_ids::sui_local_test(), 0);
        assert!(bridge_limit.sending_chain == chain_ids::eth_local_test(), 0);
        assert!(bridge_limit.limit == 10_000_000_000, 0);
    }

    #[test]
    fun test_update_asset_price_message_serialization() {
        let asset_price_message = create_update_asset_price_message<ETH>(
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
        assert!(asset_price.token_id == token_id<ETH>(), 0);
        assert!(asset_price.new_price == 12345, 0);
    }

    // Do not change/remove this test, it uses move bytes generated by Rust
    #[test]
    fun test_update_asset_price_message_serialization_regression() {
        let asset_price_message = create_update_asset_price_message<BTC>(
            chain_ids::sui_local_test(), // source chain
            266, // seq_num
            1_000_000_000 // $100k USD
        );

        // Test message serialization
        let message = serialize_message(asset_price_message);
        let expected_msg = hex::decode(
            b"0401000000000000010a0301000000003b9aca00",
        );
        assert!(message == expected_msg, 0);
        assert!(asset_price_message == deserialize_message_test_only(message), 0);

        let asset_price = extract_update_asset_price(&asset_price_message);
        assert!(asset_price.token_id == token_id<BTC>(), 0);
        assert!(asset_price.new_price == 1_000_000_000, 0);
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
        let bcs = bcs::new(input);
        assert!(peel_u64_be(&mut bcs) == expected, 0)
    }
}
