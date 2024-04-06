// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(unused_use)]
module bridge::committee {
    use std::option;
    use std::vector;

    use sui::ecdsa_k1;
    use sui::event::emit;
    use sui::tx_context::{Self, TxContext};
    use sui::vec_map::{Self, VecMap};
    use sui::vec_set;
    use sui_system::sui_system;
    use sui_system::sui_system::SuiSystemState;

    use bridge::crypto;
    use bridge::message::{Self, Blocklist, BridgeMessage};

    #[test_only]
    use sui::{hex, test_scenario, test_utils::{Self, assert_eq}};
    #[test_only]
    use bridge::chain_ids;
    #[test_only]
    use sui_system::governance_test_utils::{
        advance_epoch_with_reward_amounts,
        create_sui_system_state_for_testing,
        create_validator_for_testing
    };

    const ESignatureBelowThreshold: u64 = 0;
    const EDuplicatedSignature: u64 = 1;
    const EInvalidSignature: u64 = 2;
    const ENotSystemAddress: u64 = 3;
    const EValidatorBlocklistContainsUnknownKey: u64 = 4;
    const ESenderNotActiveValidator: u64 = 5;
    const EInvalidPubkeyLength: u64 = 6;
    const ECommitteeAlreadyInitiated: u64 = 7;
    const EDuplicatePubkey: u64 = 8;

    const SUI_MESSAGE_PREFIX: vector<u8> = b"SUI_BRIDGE_MESSAGE";

    const ECDSA_COMPRESSED_PUBKEY_LENGTH: u64 = 33;

    public struct BlocklistValidatorEvent has copy, drop {
        blocklisted: bool,
        public_keys: vector<vector<u8>>,
    }

    public struct BridgeCommittee has store {
        // commitee pub key and weight
        members: VecMap<vector<u8>, CommitteeMember>,
        // Committee member registrations for the next committee creation.
        member_registrations: VecMap<address, CommitteeMemberRegistration>,
        // Epoch when the current committee was updated,
        // the voting power for each of the committee members are snapshot from this epoch.
        // This is mainly for verification/auditing purposes, it might not be useful for bridge operations.
        last_committee_update_epoch: u64,
    }

    public struct CommitteeUpdateEvent has copy, drop {
        // commitee pub key and weight
        members: VecMap<vector<u8>, CommitteeMember>,
        stake_participation_percentage: u64
    }

    public struct CommitteeMember has copy, drop, store {
        /// The Sui Address of the validator
        sui_address: address,
        /// The public key bytes of the bridge key
        bridge_pubkey_bytes: vector<u8>,
        /// Voting power, values are voting power in the scale of 10000.
        voting_power: u64,
        /// The HTTP REST URL the member's node listens to
        /// it looks like b'https://127.0.0.1:9191'
        http_rest_url: vector<u8>,
        /// If this member is blocklisted
        blocklisted: bool,
    }

    public struct CommitteeMemberRegistration has copy, drop, store {
        /// The Sui Address of the validator
        sui_address: address,
        /// The public key bytes of the bridge key
        bridge_pubkey_bytes: vector<u8>,
        /// The HTTP REST URL the member's node listens to
        /// it looks like b'https://127.0.0.1:9191'
        http_rest_url: vector<u8>,
    }

    public(package) fun create(ctx: &TxContext): BridgeCommittee {
        assert!(tx_context::sender(ctx) == @0x0, ENotSystemAddress);
        BridgeCommittee {
            members: vec_map::empty(),
            member_registrations: vec_map::empty(),
            last_committee_update_epoch: 0,
        }
    }

    public fun verify_signatures(
        self: &BridgeCommittee,
        message: BridgeMessage,
        signatures: vector<vector<u8>>,
    ) {
        let (mut i, signature_counts) = (0, vector::length(&signatures));
        let mut seen_pub_key = vec_set::empty<vector<u8>>();
        let required_voting_power = message.required_voting_power();
        // add prefix to the message bytes
        let mut message_bytes = SUI_MESSAGE_PREFIX;
        message_bytes.append(message.serialize_message());

        let mut threshold = 0;
        while (i < signature_counts) {
            let signature = vector::borrow(&signatures, i);
            let pubkey = ecdsa_k1::secp256k1_ecrecover(signature, &message_bytes, 0);

            // check duplicate
            // and make sure pub key is part of the committee
            assert!(!seen_pub_key.contains(&pubkey), EDuplicatedSignature);
            assert!(self.members.contains(&pubkey), EInvalidSignature);

            // get committee signature weight and check pubkey is part of the committee
            let member = &self.members[&pubkey];
            if (!member.blocklisted) {
                threshold = threshold + member.voting_power;
            };
            seen_pub_key.insert(pubkey);
            i = i + 1;
        };

        assert!(threshold >= required_voting_power, ESignatureBelowThreshold);
    }

    public(package) fun register(
        self: &mut BridgeCommittee,
        system_state: &mut SuiSystemState,
        bridge_pubkey_bytes: vector<u8>,
        http_rest_url: vector<u8>,
        ctx: &TxContext
    ) {
        // We disallow registration after committee initiated in v1
        assert!(vec_map::is_empty(&self.members), ECommitteeAlreadyInitiated);
        // Ensure pubkey is valid
        assert!(vector::length(&bridge_pubkey_bytes) == ECDSA_COMPRESSED_PUBKEY_LENGTH, EInvalidPubkeyLength);
        // sender must be the same sender that created the validator object, this is to prevent DDoS from non-validator actor.
        let sender = ctx.sender();
        let validators = system_state.active_validator_addresses();

        assert!(validators.contains(&sender), ESenderNotActiveValidator);
        // Sender is active validator, record the registration

        // In case validator need to update the info
        let registration = if (self.member_registrations.contains(&sender)) {
            let registration = &mut self.member_registrations[&sender];
            registration.http_rest_url = http_rest_url;
            registration.bridge_pubkey_bytes = bridge_pubkey_bytes;
            *registration
        } else {
            let registration = CommitteeMemberRegistration {
                sui_address: sender,
                bridge_pubkey_bytes,
                http_rest_url,
            };
            self.member_registrations.insert(sender, registration);
            registration
        };

        // check uniqueness of the bridge pubkey.
        // `try_create_next_committee` will abort if bridge_pubkey_bytes are not unique and
        // that will fail the end of epoch transaction (possibly "forever", well, we
        // need to deploy proper validator changes to stop end of epoch from failing).
        check_uniqueness_bridge_keys(self, bridge_pubkey_bytes);

        emit(registration)
    }

    // Assert if `bridge_pubkey_bytes` is duplicated in `member_registrations`.
    // Dupicate keys would cause `try_create_next_committee` to fail and,
    // in consequence, an end of epoch transaction to fail (safe mode run).
    // This check will ensure the creation of the committee is correct.
    fun check_uniqueness_bridge_keys(self: &BridgeCommittee, bridge_pubkey_bytes: vector<u8>) {
        let mut count = self.member_registrations.size();
        // bridge_pubkey_bytes must be found once and once only
        let mut bridge_key_found = false;
        while (count > 0) {
            count = count - 1;
            let (_, registration) = self.member_registrations.get_entry_by_idx(count);
            if (registration.bridge_pubkey_bytes == bridge_pubkey_bytes) {
                assert!(!bridge_key_found, EDuplicatePubkey);
                bridge_key_found = true; // bridge_pubkey_bytes found, we must not have another one
            }
        };
    }

    // This method will try to create the next committee using the registration and system state,
    // if the total stake fails to meet the minimum required percentage, it will skip the update.
    // This is to ensure we don't fail the end of epoch transaction.
    public(package) fun try_create_next_committee(
        self: &mut BridgeCommittee,
        active_validator_voting_power: VecMap<address, u64>,
        min_stake_participation_percentage: u64,
        ctx: &TxContext
    ) {
        let mut i = 0;
        let mut new_members = vec_map::empty();
        let mut stake_participation_percentage = 0;

        while (i < self.member_registrations.size()) {
            // retrieve registration
            let (_, registration) = self.member_registrations.get_entry_by_idx(i);
            // Find validator stake amount from system state

            // Process registration if it's active validator
            let voting_power = active_validator_voting_power.try_get(&registration.sui_address);
            if (voting_power.is_some()) {
                let voting_power = voting_power.destroy_some();
                stake_participation_percentage = stake_participation_percentage + voting_power;

                let member = CommitteeMember {
                    sui_address: registration.sui_address,
                    bridge_pubkey_bytes: registration.bridge_pubkey_bytes,
                    voting_power: (voting_power as u64),
                    http_rest_url: registration.http_rest_url,
                    blocklisted: false,
                };

                new_members.insert(registration.bridge_pubkey_bytes, member)
            };

            i = i + 1;
        };

        // Make sure the new committee represent enough stakes, percentage are accurate to 2DP
        if (stake_participation_percentage >= min_stake_participation_percentage) {
            // Clear registrations
            self.member_registrations = vec_map::empty();
            // Store new committee info
            self.members = new_members;
            self.last_committee_update_epoch = ctx.epoch();

            emit(CommitteeUpdateEvent {
                members: new_members,
                stake_participation_percentage
            })
        }
    }

    // This function applys the blocklist to the committee members, we won't need to run this very often so this is not gas optimised.
    // TODO: add tests for this function
    public(package) fun execute_blocklist(self: &mut BridgeCommittee, blocklist: Blocklist) {
        let blocklisted = blocklist.blocklist_type() != 1;
        let eth_addresses = blocklist.blocklist_validator_addresses();
        let list_len = eth_addresses.length();
        let mut list_idx = 0;
        let mut member_idx = 0;
        let mut pub_keys = vector[];

        while (list_idx < list_len) {
            let target_address = &eth_addresses[list_idx];
            let mut found = false;

            while (member_idx < self.members.size()) {
                let (pub_key, member) = self.members.get_entry_by_idx_mut(member_idx);
                let eth_address = crypto::ecdsa_pub_key_to_eth_address(*pub_key);

                if (*target_address == eth_address) {
                    member.blocklisted = blocklisted;
                    pub_keys.push_back(*pub_key);
                    found = true;
                    member_idx = 0;
                    break
                };

                member_idx = member_idx + 1;
            };

            assert!(found, EValidatorBlocklistContainsUnknownKey);
            list_idx = list_idx + 1;
        };

        emit(BlocklistValidatorEvent {
            blocklisted,
            public_keys: pub_keys,
        })
    }

    public(package) fun committee_members(self: &BridgeCommittee): &VecMap<vector<u8>, CommitteeMember> {
        &self.members
    }

    #[test_only]
    // This is a token transfer message for testing
    const TEST_MSG: vector<u8> =
        b"00010a0000000000000000200000000000000000000000000000000000000000000000000000000000000064012000000000000000000000000000000000000000000000000000000000000000c8033930000000000000";

    #[test_only]
    const VALIDATOR1_PUBKEY: vector<u8> = b"029bef8d556d80e43ae7e0becb3a7e6838b95defe45896ed6075bb9035d06c9964";
    #[test_only]
    const VALIDATOR2_PUBKEY: vector<u8> = b"033e99a541db69bd32040dfe5037fbf5210dafa8151a71e21c5204b05d95ce0a62";
    #[test_only]
    const VALIDATOR3_PUBKEY: vector<u8> = b"033e99a541db69bd32040dfe5037fbf5210dafa8151a71e21c5204b05d95ce0a63";

    #[test]
    fun test_verify_signatures_good_path() {
        let committee = setup_test();
        let msg = message::deserialize_message_test_only(hex::decode(TEST_MSG));
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
        test_utils::destroy(committee)
    }

    #[test]
    #[expected_failure(abort_code = EDuplicatedSignature)]
    fun test_verify_signatures_duplicated_sig() {
        let committee = setup_test();
        let msg = message::deserialize_message_test_only(hex::decode(TEST_MSG));
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
        let msg = message::deserialize_message_test_only(hex::decode(TEST_MSG));
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
        let msg = message::deserialize_message_test_only(hex::decode(TEST_MSG));
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

    #[test]
    fun test_init_committee() {
        let mut scenario = test_scenario::begin(@0x0);
        let ctx = test_scenario::ctx(&mut scenario);
        let mut committee = create(ctx);

        let validators = vector[
            create_validator_for_testing(@0xA, 100, ctx),
            create_validator_for_testing(@0xC, 100, ctx)
        ];
        create_sui_system_state_for_testing(validators, 0, 0, ctx);
        advance_epoch_with_reward_amounts(0, 0, &mut scenario);
        test_scenario::next_tx(&mut scenario, @0x0);

        let mut system_state = test_scenario::take_shared<SuiSystemState>(&scenario);

        // validator registration
        register(&mut committee, &mut system_state, hex::decode(VALIDATOR1_PUBKEY), b"", &tx(@0xA, 0));
        register(&mut committee, &mut system_state, hex::decode(VALIDATOR2_PUBKEY), b"", &tx(@0xC, 0));

        // Check committee before creation
        assert!(vec_map::is_empty(&committee.members), 0);

        let ctx = test_scenario::ctx(&mut scenario);
        let voting_powers = sui_system::validator_voting_powers_for_testing(&mut system_state);
        try_create_next_committee(&mut committee, voting_powers, 6000, ctx);

        assert_eq(2, vec_map::size(&committee.members));
        let (_, member0) = vec_map::get_entry_by_idx(&committee.members, 0);
        let (_, member1) = vec_map::get_entry_by_idx(&committee.members, 1);
        assert_eq(5000, member0.voting_power);
        assert_eq(5000, member1.voting_power);

        let members = committee_members(&committee);
        assert!(members.size() == 2, 0); // must succeed

        test_utils::destroy(committee);
        test_scenario::return_shared(system_state);
        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code = ENotSystemAddress)]
    fun test_init_non_system_sender() {
        let mut scenario = test_scenario::begin(@0x1);
        let ctx = test_scenario::ctx(&mut scenario);
        let _committee = create(ctx);

        abort 0
    }

    #[test]
    #[expected_failure(abort_code = ESenderNotActiveValidator)]
    fun test_init_committee_not_validator() {
        let mut scenario = test_scenario::begin(@0x0);
        let ctx = test_scenario::ctx(&mut scenario);
        let mut committee = create(ctx);

        let validators = vector[
            create_validator_for_testing(@0xA, 100, ctx),
            create_validator_for_testing(@0xC, 100, ctx)
        ];
        create_sui_system_state_for_testing(validators, 0, 0, ctx);
        advance_epoch_with_reward_amounts(0, 0, &mut scenario);
        test_scenario::next_tx(&mut scenario, @0x0);

        let mut system_state = test_scenario::take_shared<SuiSystemState>(&scenario);

        // validator registration
        register(&mut committee, &mut system_state, hex::decode(VALIDATOR1_PUBKEY), b"", &tx(@0xD, 0));

        test_utils::destroy(committee);
        test_scenario::return_shared(system_state);
        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code = EDuplicatePubkey)]
    fun test_init_committee_dup_pubkey() {
        let mut scenario = test_scenario::begin(@0x0);
        let ctx = test_scenario::ctx(&mut scenario);
        let mut committee = create(ctx);

        let validators = vector[
            create_validator_for_testing(@0xA, 100, ctx),
            create_validator_for_testing(@0xC, 100, ctx)
        ];
        create_sui_system_state_for_testing(validators, 0, 0, ctx);
        advance_epoch_with_reward_amounts(0, 0, &mut scenario);
        test_scenario::next_tx(&mut scenario, @0x0);

        let mut system_state = test_scenario::take_shared<SuiSystemState>(&scenario);

        // validator registration
        register(&mut committee, &mut system_state, hex::decode(VALIDATOR1_PUBKEY), b"", &tx(@0xA, 0));
        register(&mut committee, &mut system_state, hex::decode(VALIDATOR1_PUBKEY), b"", &tx(@0xC, 0));

        test_utils::destroy(committee);
        test_scenario::return_shared(system_state);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_init_committee_validator_become_inactive() {
        let mut scenario = test_scenario::begin(@0x0);
        let ctx = test_scenario::ctx(&mut scenario);
        let mut committee = create(ctx);

        let validators = vector[
            create_validator_for_testing(@0xA, 100, ctx),
            create_validator_for_testing(@0xC, 100, ctx),
            create_validator_for_testing(@0xD, 100, ctx),
            create_validator_for_testing(@0xE, 100, ctx),
            create_validator_for_testing(@0xF, 100, ctx)
        ];
        create_sui_system_state_for_testing(validators, 0, 0, ctx);
        advance_epoch_with_reward_amounts(0, 0, &mut scenario);
        test_scenario::next_tx(&mut scenario, @0x0);

        let mut system_state = test_scenario::take_shared<SuiSystemState>(&scenario);

        // validator registration, 3 validators registered, should have 60% voting power in total
        register(&mut committee, &mut system_state, hex::decode(VALIDATOR1_PUBKEY), b"", &tx(@0xA, 0));
        register(&mut committee, &mut system_state, hex::decode(VALIDATOR2_PUBKEY), b"", &tx(@0xC, 0));
        register(&mut committee, &mut system_state, hex::decode(VALIDATOR3_PUBKEY), b"", &tx(@0xD, 0));

        // Verify validator registration
        assert_eq(3, vec_map::size(&committee.member_registrations));

        // Validator 0xA become inactive, total voting power become 50%
        sui_system::request_remove_validator(&mut system_state, &mut tx(@0xA, 0));
        test_scenario::return_shared(system_state);
        advance_epoch_with_reward_amounts(0, 0, &mut scenario);

        let mut system_state = test_scenario::take_shared<SuiSystemState>(&scenario);

        // create committee should not create a committe because of not enough stake.
        let ctx = test_scenario::ctx(&mut scenario);
        let voting_powers = sui_system::validator_voting_powers_for_testing(&mut system_state);
        try_create_next_committee(&mut committee, voting_powers, 6000, ctx);

        assert!(vec_map::is_empty(&committee.members), 0);

        test_utils::destroy(committee);
        test_scenario::return_shared(system_state);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_update_committee_registration() {
        let mut scenario = test_scenario::begin(@0x0);
        let ctx = test_scenario::ctx(&mut scenario);
        let mut committee = create(ctx);

        let validators = vector[
            create_validator_for_testing(@0xA, 100, ctx),
            create_validator_for_testing(@0xC, 100, ctx)
        ];
        create_sui_system_state_for_testing(validators, 0, 0, ctx);
        advance_epoch_with_reward_amounts(0, 0, &mut scenario);
        test_scenario::next_tx(&mut scenario, @0x0);

        let mut system_state = test_scenario::take_shared<SuiSystemState>(&scenario);

        // validator registration
        register(&mut committee, &mut system_state, hex::decode(VALIDATOR1_PUBKEY), b"", &tx(@0xA, 0));

        // Verify registration info
        assert_eq(1, vec_map::size(&committee.member_registrations));
        let (address, registration) = vec_map::get_entry_by_idx(&committee.member_registrations, 0);
        assert_eq(@0xA, *address);
        assert_eq(hex::decode(VALIDATOR1_PUBKEY), registration.bridge_pubkey_bytes);

        // Register again with different pub key.
        register(&mut committee, &mut system_state, hex::decode(VALIDATOR2_PUBKEY), b"", &tx(@0xA, 0));

        // Verify registration info, registration count should still be 1
        assert_eq(1, vec_map::size(&committee.member_registrations));
        let (address, registration) = vec_map::get_entry_by_idx(&committee.member_registrations, 0);
        assert_eq(@0xA, *address);
        assert_eq(hex::decode(VALIDATOR2_PUBKEY), registration.bridge_pubkey_bytes);

        // teardown
        test_utils::destroy(committee);
        test_scenario::return_shared(system_state);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_init_committee_not_enough_stake() {
        let mut scenario = test_scenario::begin(@0x0);
        let ctx = test_scenario::ctx(&mut scenario);
        let mut committee = create(ctx);

        let validators = vector[
            create_validator_for_testing(@0xA, 100, ctx),
            create_validator_for_testing(@0xC, 100, ctx)
        ];
        create_sui_system_state_for_testing(validators, 0, 0, ctx);
        advance_epoch_with_reward_amounts(0, 0, &mut scenario);
        test_scenario::next_tx(&mut scenario, @0x0);

        let mut system_state = test_scenario::take_shared<SuiSystemState>(&scenario);

        // validator registration
        register(&mut committee, &mut system_state, hex::decode(VALIDATOR1_PUBKEY), b"", &tx(@0xA, 0));

        // Check committee before creation
        assert!(vec_map::is_empty(&committee.members), 0);

        let ctx = test_scenario::ctx(&mut scenario);
        let voting_powers = sui_system::validator_voting_powers_for_testing(&mut system_state);
        try_create_next_committee(&mut committee, voting_powers, 6000, ctx);

        // committee should be empty because registration did not reach min stake threshold.
        assert!(vec_map::is_empty(&committee.members), 0);

        test_utils::destroy(committee);
        test_scenario::return_shared(system_state);
        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code = ECommitteeAlreadyInitiated)]
    fun test_register_already_initialized() {
        let mut scenario = test_scenario::begin(@0x0);
        let ctx = test_scenario::ctx(&mut scenario);
        let mut committee = create(ctx);

        let validators = vector[
            create_validator_for_testing(@0xA, 100, ctx),
            create_validator_for_testing(@0xC, 100, ctx)
        ];
        create_sui_system_state_for_testing(validators, 0, 0, ctx);
        advance_epoch_with_reward_amounts(0, 0, &mut scenario);

        test_scenario::next_tx(&mut scenario, @0x0);
        let mut system_state = test_scenario::take_shared<SuiSystemState>(&scenario);
        register(&mut committee, &mut system_state, hex::decode(VALIDATOR1_PUBKEY), b"", &tx(@0xA, 0));
        register(&mut committee, &mut system_state, hex::decode(VALIDATOR2_PUBKEY), b"", &tx(@0xC, 0));
        assert!(vec_map::is_empty(&committee.members), 0);
        let ctx = test_scenario::ctx(&mut scenario);
        let voting_powers = sui_system::validator_voting_powers_for_testing(&mut system_state);
        try_create_next_committee(&mut committee, voting_powers, 6000, ctx);

        test_scenario::next_tx(&mut scenario, @0x0);
        assert!(vec_map::size(&committee.members) == 2, 1000); // must succeed
        // this fails because committee is already initiated
        register(&mut committee, &mut system_state, hex::decode(VALIDATOR1_PUBKEY), b"", &tx(@0xA, 0));

        abort 0
    }

    #[test]
    #[expected_failure(abort_code = EInvalidPubkeyLength)]
    fun test_register_bad_pubkey() {
        let mut scenario = test_scenario::begin(@0x0);
        let ctx = test_scenario::ctx(&mut scenario);
        let mut committee = create(ctx);

        let validators = vector[
            create_validator_for_testing(@0xA, 100, ctx),
            create_validator_for_testing(@0xC, 100, ctx)
        ];
        create_sui_system_state_for_testing(validators, 0, 0, ctx);
        advance_epoch_with_reward_amounts(0, 0, &mut scenario);

        test_scenario::next_tx(&mut scenario, @0x0);
        let mut system_state = test_scenario::take_shared<SuiSystemState>(&scenario);
        register(&mut committee, &mut system_state, hex::decode(VALIDATOR2_PUBKEY), b"", &tx(@0xC, 0));
        // this fails with invalid public key
        register(&mut committee, &mut system_state, b"029bef8", b"", &tx(@0xA, 0));

        abort 0
    }


    #[test_only]
    fun tx(sender: address, hint: u64): TxContext {
        tx_context::new_from_hint(sender, hint, 1, 0, 0)
    }

    #[test]
    #[expected_failure(abort_code = ESignatureBelowThreshold)]
    fun test_verify_signatures_with_blocked_committee_member() {
        let mut committee = setup_test();
        let msg = message::deserialize_message_test_only(hex::decode(TEST_MSG));
        // good path, this test should have passed in previous test
        verify_signatures(
            &committee,
            msg,
            vector[hex::decode(
                b"8ba030a450cb1e36f61e572645fc9da1dea5f79b6db663a21ab63286d7fc29af447433abdd0c0b35ab751154ac5b612ae64d3be810f0d9e10ff68e764514ced300"
            ), hex::decode(
                b"439379cc7b3ee3ebe1ff59d011dafc1caac47da6919b089c90f6a24e8c284b963b20f1f5421385456e57ac6b69c4b5f0d345aa09b8bc96d88d87051c7349e83801"
            )],
        );

        let (validator1, member) = vec_map::get_entry_by_idx(&committee.members, 0);
        assert!(!member.blocklisted, 0);

        // Block a member
        let blocklist = message::create_blocklist_message(
            chain_ids::sui_testnet(),
            0,
            0, // type 0 is block
            vector[crypto::ecdsa_pub_key_to_eth_address(*validator1)]
        );
        let blocklist = message::extract_blocklist_payload(&blocklist);
        execute_blocklist(&mut committee, blocklist);

        let (_, blocked_member) = vec_map::get_entry_by_idx(&committee.members, 0);
        assert!(blocked_member.blocklisted, 0);

        // Verify signature should fail now
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
        test_utils::destroy(committee);
    }

    #[test]
    #[expected_failure(abort_code = EValidatorBlocklistContainsUnknownKey)]
    fun test_execute_blocklist_abort_upon_unknown_validator() {
        let mut committee = setup_test();

        // // val0 and val1 are not blocked yet
        let (validator0, _) = vec_map::get_entry_by_idx(&committee.members, 0);
        // assert!(!member0.blocklisted, 0);
        // let (validator1, member1) = vec_map::get_entry_by_idx(&committee.members, 1);
        // assert!(!member1.blocklisted, 0);

        let eth_address0 = crypto::ecdsa_pub_key_to_eth_address(*validator0);
        let invalid_eth_address1 = x"0000000000000000000000000000000000000000";

        // Blocklist both
        let blocklist = message::create_blocklist_message(
            chain_ids::sui_testnet(),
            0, // seq
            0, // type 0 is blocklist
            vector[eth_address0, invalid_eth_address1]
        );
        let blocklist = message::extract_blocklist_payload(&blocklist);
        execute_blocklist(&mut committee, blocklist);

        // Clean up
        test_utils::destroy(committee);
    }

    #[test]
    fun test_execute_blocklist() {
        let mut committee = setup_test();

        // val0 and val1 are not blocked yet
        let (validator0, member0) = vec_map::get_entry_by_idx(&committee.members, 0);
        assert!(!member0.blocklisted, 0);
        let (validator1, member1) = vec_map::get_entry_by_idx(&committee.members, 1);
        assert!(!member1.blocklisted, 0);

        let eth_address0 = crypto::ecdsa_pub_key_to_eth_address(*validator0);
        let eth_address1 = crypto::ecdsa_pub_key_to_eth_address(*validator1);

        // Blocklist both
        let blocklist = message::create_blocklist_message(
            chain_ids::sui_testnet(),
            0, // seq
            0, // type 0 is blocklist
            vector[eth_address0, eth_address1]
        );
        let blocklist = message::extract_blocklist_payload(&blocklist);
        execute_blocklist(&mut committee, blocklist);

        // Blocklist both reverse order
        let blocklist = message::create_blocklist_message(
            chain_ids::sui_testnet(),
            0, // seq
            0, // type 0 is blocklist
            vector[eth_address1, eth_address0]
        );
        let blocklist = message::extract_blocklist_payload(&blocklist);
        execute_blocklist(&mut committee, blocklist);

        // val 0 is blocklisted
        let (_, blocked_member) = vec_map::get_entry_by_idx(&committee.members, 0);
        assert!(blocked_member.blocklisted, 0);
        // val 1 is too
        let (_, blocked_member) = vec_map::get_entry_by_idx(&committee.members, 1);
        assert!(blocked_member.blocklisted, 0);

        // unblocklist val1
        let blocklist = message::create_blocklist_message(
            chain_ids::sui_testnet(),
            1, // seq, this is supposed to increment, but we don't test it here
            1, // type 1 is unblocklist
            vector[eth_address1],
        );
        let blocklist = message::extract_blocklist_payload(&blocklist);
        execute_blocklist(&mut committee, blocklist);

        // val 0 is still blocklisted
        let (_, blocked_member) = vec_map::get_entry_by_idx(&committee.members, 0);
        assert!(blocked_member.blocklisted, 0);
        // val 1 is not
        let (_, blocked_member) = vec_map::get_entry_by_idx(&committee.members, 1);
        assert!(!blocked_member.blocklisted, 0);

        // Clean up
        test_utils::destroy(committee);
    }

    #[test_only]
    fun setup_test(): BridgeCommittee {
        let mut members = vec_map::empty<vector<u8>, CommitteeMember>();

        let bridge_pubkey_bytes = hex::decode(VALIDATOR1_PUBKEY);
        vec_map::insert(&mut members, bridge_pubkey_bytes, CommitteeMember {
            sui_address: @0xA,
            bridge_pubkey_bytes,
            voting_power: 3333,
            http_rest_url: b"https://127.0.0.1:9191",
            blocklisted: false
        });

        let bridge_pubkey_bytes = hex::decode(VALIDATOR2_PUBKEY);
        vec_map::insert(&mut members, bridge_pubkey_bytes, CommitteeMember {
            sui_address: @0xC,
            bridge_pubkey_bytes,
            voting_power: 3333,
            http_rest_url: b"https://127.0.0.1:9192",
            blocklisted: false
        });

        let committee = BridgeCommittee {
            members,
            member_registrations: vec_map::empty(),
            last_committee_update_epoch: 1,
        };
        committee
    }
}
