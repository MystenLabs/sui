// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(unused_use)]
module bridge::committee {
    use std::vector;

    use sui::address;
    use sui::ecdsa_k1;
    use sui::hex;
    use sui::tx_context::{Self, TxContext};
    use sui::vec_map::{Self, VecMap};
    use sui::vec_set;

    use bridge::message::{Self, BridgeMessage};
    use bridge::message_types;

    friend bridge::bridge;

    const ESignatureBelowThreshold: u64 = 0;
    const EDuplicatedSignature: u64 = 1;
    const EInvalidSignature: u64 = 2;
    // const ENotSystemAddress: u64 = 3;

    const SUI_MESSAGE_PREFIX: vector<u8> = b"SUI_BRIDGE_MESSAGE";

    struct BridgeCommittee has store {
        // commitee pub key and weight
        members: VecMap<vector<u8>, CommitteeMember>,
        // threshold for each message type
        thresholds: VecMap<u8, u64>
    }

    struct CommitteeMember has drop, store {
        /// The Sui Address of the validator
        sui_address: address,
        /// The public key bytes of the bridge key
        bridge_pubkey_bytes: vector<u8>,
        /// Voting power
        voting_power: u64,
        /// The HTTP REST URL the member's node listens to
        /// it looks like b'https://127.0.0.1:9191'
        http_rest_url: vector<u8>,
        /// If this member is blocklisted
        blocklisted: bool,
    }

    public(friend) fun create(_ctx: &TxContext): BridgeCommittee {
        // assert!(tx_context::sender(ctx) == @0x0, ENotSystemAddress);
        // Hardcoded genesis committee
        // TODO: change this to real committe members
        let members = vec_map::empty<vector<u8>, CommitteeMember>();

        let bridge_pubkey_bytes = hex::decode(b"02321ede33d2c2d7a8a152f275a1484edef2098f034121a602cb7d767d38680aa4");
        vec_map::insert(&mut members, bridge_pubkey_bytes, CommitteeMember {
            // TODO: why do we need sui_address?
            sui_address: address::from_u256(1),
            bridge_pubkey_bytes,
            voting_power: 2500,
            http_rest_url: b"http://127.0.0.1:9191",
            blocklisted: false
        });

        let bridge_pubkey_bytes = hex::decode(b"027f1178ff417fc9f5b8290bd8876f0a157a505a6c52db100a8492203ddd1d4279");
        vec_map::insert(&mut members, bridge_pubkey_bytes, CommitteeMember {
            sui_address: address::from_u256(2),
            bridge_pubkey_bytes,
            voting_power: 2500,
            http_rest_url: b"http://127.0.0.1:9192",
            blocklisted: false
        });

        let bridge_pubkey_bytes = hex::decode(b"026f311bcd1c2664c14277c7a80e4857c690626597064f89edc33b8f67b99c6bc0");
        vec_map::insert(&mut members, bridge_pubkey_bytes, CommitteeMember {
            sui_address: address::from_u256(3),
            bridge_pubkey_bytes,
            voting_power: 2500,
            http_rest_url: b"http://127.0.0.1:9193",
            blocklisted: false
        });

        let bridge_pubkey_bytes = hex::decode(b"03a57b85771aedeb6d31c808be9a6e73194e4b70e679608f2bca68bcc684773736");
        vec_map::insert(&mut members, bridge_pubkey_bytes, CommitteeMember {
            sui_address: address::from_u256(4),
            bridge_pubkey_bytes,
            voting_power: 2500,
            http_rest_url: b"http://127.0.0.1:9194",
            blocklisted: false
        });

        let thresholds = vec_map::empty();
        vec_map::insert(&mut thresholds, message_types::token(), 3334);
        BridgeCommittee { members, thresholds }
    }

    public fun verify_signatures(
        self: &BridgeCommittee,
        message: BridgeMessage,
        signatures: vector<vector<u8>>,
    ) {
        let (i, signature_counts) = (0, vector::length(&signatures));
        let seen_pub_key = vec_set::empty<vector<u8>>();
        let required_threshold = *vec_map::get(&self.thresholds, &message::message_type(&message));

        // add prefix to the message bytes
        let message_bytes = SUI_MESSAGE_PREFIX;
        vector::append(&mut message_bytes, message::serialize_message(message));

        let threshold = 0;
        while (i < signature_counts) {
            let signature = vector::borrow(&signatures, i);
            let pubkey = ecdsa_k1::secp256k1_ecrecover(signature, &message_bytes, 0);
            // check duplicate
            assert!(!vec_set::contains(&seen_pub_key, &pubkey), EDuplicatedSignature);
            // make sure pub key is part of the committee
            assert!(vec_map::contains(&self.members, &pubkey), EInvalidSignature);
            // get committee signature weight and check pubkey is part of the committee
            let member = vec_map::get(&self.members, &pubkey);
            if (!member.blocklisted) {
                threshold = threshold + member.voting_power;
            };
            i = i + 1;
            vec_set::insert(&mut seen_pub_key, pubkey);
        };
        assert!(threshold >= required_threshold, ESignatureBelowThreshold);
    }

    #[test_only]
    const TEST_MSG: vector<u8> =
        b"00010a0000000000000000200000000000000000000000000000000000000000000000000000000000000064012000000000000000000000000000000000000000000000000000000000000000c8033930000000000000";

    #[test]
    fun test_verify_signatures_good_path() {
        let committee = setup_test();
        let msg = message::deserialize_message(hex::decode(TEST_MSG));
        // good path
        verify_signatures(
            &committee,
            msg,
            vector[hex::decode(
                b"8ba030a450cb1e36f61e572645fc9da1dea5f79b6db663a21ab63286d7fc29af447433abdd0c0b35ab751154ac5b612ae64d3be810f0d9e10ff68e764514ced300"
            ), hex::decode(
                b"439379cc7b3ee3ebe1ff59d011dafc1caac47da6919b089c90f6a24e8c284b963b20f1f5421385456e57ac6b69c4b5f0d345aa09b8bc96d88d87051c7349e83801"
            )],
        );

        // Clean up
        let BridgeCommittee {
            members: _,
            thresholds: _
        } = committee;
    }

    #[test]
    #[expected_failure(abort_code = EDuplicatedSignature)]
    fun test_verify_signatures_duplicated_sig() {
        let committee = setup_test();
        let msg = message::deserialize_message(hex::decode(TEST_MSG));
        // good path
        verify_signatures(
            &committee,
            msg,
            vector[hex::decode(
                b"439379cc7b3ee3ebe1ff59d011dafc1caac47da6919b089c90f6a24e8c284b963b20f1f5421385456e57ac6b69c4b5f0d345aa09b8bc96d88d87051c7349e83801"
            ), hex::decode(
                b"439379cc7b3ee3ebe1ff59d011dafc1caac47da6919b089c90f6a24e8c284b963b20f1f5421385456e57ac6b69c4b5f0d345aa09b8bc96d88d87051c7349e83801"
            )],
        );
        abort 0
    }

    #[test]
    #[expected_failure(abort_code = EInvalidSignature)]
    fun test_verify_signatures_invalid_signature() {
        let committee = setup_test();
        let msg = message::deserialize_message(hex::decode(TEST_MSG));
        // good path
        verify_signatures(
            &committee,
            msg,
            vector[hex::decode(
                b"6ffb3e5ce04dd138611c49520fddfbd6778879c2db4696139f53a487043409536c369c6ffaca165ce3886723cfa8b74f3e043e226e206ea25e313ea2215e6caf01"
            )],
        );
        abort 0
    }

    #[test]
    #[expected_failure(abort_code = ESignatureBelowThreshold)]
    fun test_verify_signatures_below_threshold() {
        let committee = setup_test();
        let msg = message::deserialize_message(hex::decode(TEST_MSG));
        // good path
        verify_signatures(
            &committee,
            msg,
            vector[hex::decode(
                b"439379cc7b3ee3ebe1ff59d011dafc1caac47da6919b089c90f6a24e8c284b963b20f1f5421385456e57ac6b69c4b5f0d345aa09b8bc96d88d87051c7349e83801"
            )],
        );
        abort 0
    }

    #[test_only]
    fun setup_test(): BridgeCommittee {
        let members = vec_map::empty<vector<u8>, CommitteeMember>();

        let bridge_pubkey_bytes = hex::decode(b"029bef8d556d80e43ae7e0becb3a7e6838b95defe45896ed6075bb9035d06c9964");
        vec_map::insert(&mut members, bridge_pubkey_bytes, CommitteeMember {
            sui_address: address::from_u256(1),
            bridge_pubkey_bytes,
            voting_power: 100,
            http_rest_url: b"https://127.0.0.1:9191",
            blocklisted: false
        });

        let bridge_pubkey_bytes = hex::decode(b"033e99a541db69bd32040dfe5037fbf5210dafa8151a71e21c5204b05d95ce0a62");
        vec_map::insert(&mut members, bridge_pubkey_bytes, CommitteeMember {
            sui_address: address::from_u256(2),
            bridge_pubkey_bytes,
            voting_power: 100,
            http_rest_url: b"https://127.0.0.1:9192",
            blocklisted: false
        });

        let thresholds = vec_map::empty<u8, u64>();
        vec_map::insert(&mut thresholds, message_types::token(), 200);

        let committee = BridgeCommittee {
            members,
            thresholds
        };
        committee
    }
}