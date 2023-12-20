// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module bridge::message {
    use std::vector;

    use sui::bcs;

    use bridge::message_types;

    #[test_only]
    use bridge::chain_ids;
    #[test_only]
    use bridge::treasury::token_id;
    #[test_only]
    use bridge::usdc::USDC;
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

    const CURRENT_MESSAGE_VERSION: u8 = 1;

    struct BridgeMessage has copy, store, drop {
        message_type: u8,
        message_version: u8,
        seq_num: u64,
        payload: vector<u8>
    }

    struct BridgeMessageKey has copy, drop, store {
        source_chain: u8,
        message_type: u8,
        bridge_seq_num: u64
    }

    struct TokenPayload has drop {
        source_chain: u8,
        sender_address: vector<u8>,
        target_chain: u8,
        target_address: vector<u8>,
        token_type: u8,
        amount: u64
    }

    struct EmergencyOp has drop {
        op_type: u8
    }

    public fun extract_token_bridge_payload(message: &BridgeMessage): TokenPayload {
        let bcs = bcs::new(message.payload);
        let source_chain = bcs::peel_u8(&mut bcs);
        let sender_address = bcs::peel_vec_u8(&mut bcs);
        let target_chain = bcs::peel_u8(&mut bcs);
        let target_address = bcs::peel_vec_u8(&mut bcs);
        let token_type = bcs::peel_u8(&mut bcs);
        let amount = bcs::peel_u64(&mut bcs);
        TokenPayload {
            source_chain,
            sender_address,
            target_chain,
            target_address,
            token_type,
            amount
        }
    }

    public fun extract_emergency_op_payload(message: &BridgeMessage): EmergencyOp {
        let bcs = bcs::new(message.payload);
        EmergencyOp {
            op_type: bcs::peel_u8(&mut bcs)
        }
    }

    public fun serialise_message(message: BridgeMessage): vector<u8> {
        let BridgeMessage {
            message_type,
            message_version: version,
            seq_num: bridge_seq_num,
            payload
        } = message;

        let message = vector[];
        vector::push_back(&mut message, message_type);
        vector::push_back(&mut message, version);
        vector::append(&mut message, bcs::to_bytes(&bridge_seq_num));
        vector::append(&mut message, payload);
        message
    }

    public fun create_token_bridge_message(
        source_chain: u8,
        seq_num: u64,
        sender_address: vector<u8>,
        target_chain: u8,
        target_address: vector<u8>,
        token_type: u8,
        amount: u64
    ): BridgeMessage {
        BridgeMessage {
            message_type: message_types::token(),
            message_version: CURRENT_MESSAGE_VERSION,
            seq_num,
            payload: bcs::to_bytes(&TokenPayload {
                source_chain,
                sender_address,
                target_chain,
                target_address,
                token_type,
                amount
            })
        }
    }

    public fun create_emergency_op_message(
        seq_num: u64,
        op_type: u8,
    ): BridgeMessage {
        BridgeMessage {
            message_type: message_types::emergency_op(),
            message_version: CURRENT_MESSAGE_VERSION,
            seq_num,
            payload: bcs::to_bytes(&EmergencyOp { op_type })
        }
    }

    public fun create_key(source_chain: u8, message_type: u8, bridge_seq_num: u64): BridgeMessageKey {
        BridgeMessageKey { source_chain, message_type, bridge_seq_num }
    }

    public fun key(self: &BridgeMessage): BridgeMessageKey {
        let source_chain = if (self.message_type == message_types::token()) {
            let bcs = bcs::new(self.payload);
            bcs::peel_u8(&mut bcs)
        }else {
            0
        };
        create_key(source_chain, self.message_type, self.seq_num)
    }

    // BridgeMessage getters
    public fun message_type(self: &BridgeMessage): u8 {
        self.message_type
    }

    public fun seq_num(self: &BridgeMessage): u64 {
        self.seq_num
    }

    // TokenBridgePayload getters
    public fun token_source_chain(self: &TokenPayload): u8 {
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

    #[test_only]
    public fun deserialise_message(message: vector<u8>): BridgeMessage {
        let bcs = bcs::new(message);
        BridgeMessage {
            message_type: bcs::peel_u8(&mut bcs),
            message_version: bcs::peel_u8(&mut bcs),
            seq_num: bcs::peel_u64(&mut bcs),
            payload: bcs::into_remainder_bytes(bcs)
        }
    }

    #[test]
    fun test_message_serialisation() {
        let sender_address = address::from_u256(100);
        let scenario = test_scenario::begin(sender_address);
        let ctx = test_scenario::ctx(&mut scenario);

        let coin = coin::mint_for_testing<USDC>(12345, ctx);

        let token_bridge_message = BridgeMessage {
            message_type: message_types::token(),
            message_version: 1,
            seq_num: 10,
            payload: bcs::to_bytes(&TokenPayload {
                source_chain: chain_ids::sui_testnet(),
                sender_address: address::to_bytes(sender_address),
                target_chain: chain_ids::eth_sepolia(),
                target_address: address::to_bytes(address::from_u256(200)),
                token_type: token_id<USDC>(),
                amount: balance::value(coin::balance(&coin))
            })
        };

        let message = serialise_message(token_bridge_message);

        let expected_msg = hex::decode(
            b"00010a00000000000000012000000000000000000000000000000000000000000000000000000000000000640b2000000000000000000000000000000000000000000000000000000000000000c8033930000000000000",
        );

        assert!(message == expected_msg, 0);
        assert!(token_bridge_message == deserialise_message(message), 0);

        coin::burn_for_testing(coin);
        test_scenario::end(scenario);
    }
}
