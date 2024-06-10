// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module bridge::committee_test {

    use sui::vec_map;
    use sui_system::sui_system;
    use sui_system::sui_system::SuiSystemState;

    use bridge::committee::{
        BridgeCommittee, CommitteeMember, blocklisted, bridge_pubkey_bytes, create,
        members, member_registrations,
        register, try_create_next_committee, verify_signatures, voting_power,
    };
    use bridge::committee::execute_blocklist;
    use bridge::committee::make_committee_member;
    use bridge::committee::make_bridge_committee;
    use bridge::crypto;
    use bridge::message;

    use sui::{hex, test_scenario, test_utils::{Self, assert_eq}};
    use bridge::chain_ids;
    use sui_system::governance_test_utils::{
        advance_epoch_with_reward_amounts,
        create_sui_system_state_for_testing,
        create_validator_for_testing
    };

    // This is a token transfer message for testing
    const TEST_MSG: vector<u8> =
        b"00010a0000000000000000200000000000000000000000000000000000000000000000000000000000000064012000000000000000000000000000000000000000000000000000000000000000c8033930000000000000";

    const VALIDATOR1_PUBKEY: vector<u8> = b"029bef8d556d80e43ae7e0becb3a7e6838b95defe45896ed6075bb9035d06c9964";
    const VALIDATOR2_PUBKEY: vector<u8> = b"033e99a541db69bd32040dfe5037fbf5210dafa8151a71e21c5204b05d95ce0a62";
    const VALIDATOR3_PUBKEY: vector<u8> = b"033e99a541db69bd32040dfe5037fbf5210dafa8151a71e21c5204b05d95ce0a63";

    #[test]
    fun test_verify_signatures_good_path() {
        let committee = setup_test();
        let msg = message::deserialize_message_test_only(hex::decode(TEST_MSG));
        // good path
        committee.verify_signatures(
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
    #[expected_failure(abort_code = bridge::committee::EDuplicatedSignature)]
    fun test_verify_signatures_duplicated_sig() {
        let committee = setup_test();
        let msg = message::deserialize_message_test_only(hex::decode(TEST_MSG));
        // good path
        committee.verify_signatures(
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
    #[expected_failure(abort_code = bridge::committee::EInvalidSignature)]
    fun test_verify_signatures_invalid_signature() {
        let committee = setup_test();
        let msg = message::deserialize_message_test_only(hex::decode(TEST_MSG));
        // good path
        committee.verify_signatures(
            msg,
            vector[hex::decode(
                b"6ffb3e5ce04dd138611c49520fddfbd6778879c2db4696139f53a487043409536c369c6ffaca165ce3886723cfa8b74f3e043e226e206ea25e313ea2215e6caf01"
            )],
        );
        abort 0
    }

    #[test]
    #[expected_failure(abort_code = bridge::committee::ESignatureBelowThreshold)]
    fun test_verify_signatures_below_threshold() {
        let committee = setup_test();
        let msg = message::deserialize_message_test_only(hex::decode(TEST_MSG));
        // good path
        committee.verify_signatures(
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
        committee.register(
            &mut system_state,
            hex::decode(VALIDATOR1_PUBKEY),
            b"",
            &tx(@0xA, 0),
        );
        committee.register(
            &mut system_state,
            hex::decode(VALIDATOR2_PUBKEY),
            b"",
            &tx(@0xC, 0),
        );

        // Check committee before creation
        assert!(committee.members().is_empty());

        let ctx = test_scenario::ctx(&mut scenario);
        let voting_powers = system_state.validator_voting_powers_for_testing();
        committee.try_create_next_committee(voting_powers, 6000, ctx);

        assert_eq(2, committee.members().size());
        let (_, member0) = committee.members().get_entry_by_idx(0);
        let (_, member1) = committee.members().get_entry_by_idx(1);
        assert_eq(5000, member0.voting_power());
        assert_eq(5000, member1.voting_power());

        let members = committee.members();
        assert!(members.size() == 2); // must succeed

        test_utils::destroy(committee);
        test_scenario::return_shared(system_state);
        test_scenario::end(scenario);
    }

    #[test]
    fun test_update_node_url() {
        let mut scenario = test_scenario::begin(@0x0);
        let ctx = test_scenario::ctx(&mut scenario);
        let mut committee = create(ctx);

        let validators = vector[
            create_validator_for_testing(@0xA, 100, ctx),
        ];
        create_sui_system_state_for_testing(validators, 0, 0, ctx);
        advance_epoch_with_reward_amounts(0, 0, &mut scenario);
        test_scenario::next_tx(&mut scenario, @0x0);

        let mut system_state = test_scenario::take_shared<SuiSystemState>(&scenario);

        // validator registration
        committee.register(
            &mut system_state,
            hex::decode(VALIDATOR1_PUBKEY),
            b"test url 1",
            &tx(@0xA, 0),
        );

        let ctx = test_scenario::ctx(&mut scenario);
        let voting_powers = system_state.validator_voting_powers_for_testing();
        committee.try_create_next_committee(voting_powers, 6000, ctx);

        let members = committee.members();
        assert!(members.size() == 1);
        let (_, member) = members.get_entry_by_idx(0);
        assert_eq(member.http_rest_url(), b"test url 1");

        // Update URL
        committee.update_node_url(
            b"test url 2",
            &tx(@0xA, 0),
        );

        let members = committee.members();
        let (_, member) = members.get_entry_by_idx(0);
        assert_eq(member.http_rest_url(), b"test url 2");

        test_utils::destroy(committee);
        test_scenario::return_shared(system_state);
        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code = bridge::committee::ESenderIsNotInBridgeCommittee)]
    fun test_update_node_url_not_validator() {
        let mut scenario = test_scenario::begin(@0x0);
        let ctx = test_scenario::ctx(&mut scenario);
        let mut committee = create(ctx);

        let validators = vector[
            create_validator_for_testing(@0xA, 100, ctx),
        ];
        create_sui_system_state_for_testing(validators, 0, 0, ctx);
        advance_epoch_with_reward_amounts(0, 0, &mut scenario);
        test_scenario::next_tx(&mut scenario, @0x0);

        let mut system_state = test_scenario::take_shared<SuiSystemState>(&scenario);

        // validator registration
        committee.register(
            &mut system_state,
            hex::decode(VALIDATOR1_PUBKEY),
            b"test url 1",
            &tx(@0xA, 0),
        );

        let ctx = test_scenario::ctx(&mut scenario);
        let voting_powers = system_state.validator_voting_powers_for_testing();
        committee.try_create_next_committee(voting_powers, 6000, ctx);

        // Update URL should fail for validator @0xB
        committee.update_node_url(
            b"test url",
            &tx(@0xB, 0),
        );

        // test should have failed, abort
        abort 0
    }

    #[test]
    #[expected_failure(abort_code = bridge::committee::ENotSystemAddress)]
    fun test_init_non_system_sender() {
        let mut scenario = test_scenario::begin(@0x1);
        let ctx = test_scenario::ctx(&mut scenario);
        let _committee = create(ctx);

        abort 0
    }

    #[test]
    #[expected_failure(abort_code = bridge::committee::ESenderNotActiveValidator)]
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
        committee.register(&mut system_state, hex::decode(VALIDATOR1_PUBKEY), b"", &tx(@0xD, 0));

        test_utils::destroy(committee);
        test_scenario::return_shared(system_state);
        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code = bridge::committee::EDuplicatePubkey)]
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
        committee.register(&mut system_state, hex::decode(VALIDATOR1_PUBKEY), b"", &tx(@0xA, 0));
        committee.register(&mut system_state, hex::decode(VALIDATOR1_PUBKEY), b"", &tx(@0xC, 0));

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
        committee.register(&mut system_state, hex::decode(VALIDATOR1_PUBKEY), b"", &tx(@0xA, 0));
        committee.register(&mut system_state, hex::decode(VALIDATOR2_PUBKEY), b"", &tx(@0xC, 0));
        committee.register(&mut system_state, hex::decode(VALIDATOR3_PUBKEY), b"", &tx(@0xD, 0));

        // Verify validator registration
        assert_eq(3, committee.member_registrations().size());

        // Validator 0xA become inactive, total voting power become 50%
        sui_system::request_remove_validator(&mut system_state, &mut tx(@0xA, 0));
        test_scenario::return_shared(system_state);
        advance_epoch_with_reward_amounts(0, 0, &mut scenario);

        let mut system_state = test_scenario::take_shared<SuiSystemState>(&scenario);

        // create committee should not create a committe because of not enough stake.
        let ctx = test_scenario::ctx(&mut scenario);
        let voting_powers = sui_system::validator_voting_powers_for_testing(&mut system_state);
        try_create_next_committee(&mut committee, voting_powers, 6000, ctx);

        assert!(committee.members().is_empty());

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
        committee.register(&mut system_state, hex::decode(VALIDATOR1_PUBKEY), b"", &tx(@0xA, 0));

        // Verify registration info
        assert_eq(1, committee.member_registrations().size());
        let (address, registration) = committee.member_registrations().get_entry_by_idx(0);
        assert_eq(@0xA, *address);
        assert!(
            &hex::decode(VALIDATOR1_PUBKEY) == registration.bridge_pubkey_bytes(),
            0,
        );

        // Register again with different pub key.
        committee.register(&mut system_state, hex::decode(VALIDATOR2_PUBKEY), b"", &tx(@0xA, 0));

        // Verify registration info, registration count should still be 1
        assert_eq(1, committee.member_registrations().size());
        let (address, registration) = committee.member_registrations().get_entry_by_idx(0);
        assert_eq(@0xA, *address);
        assert!(
            &hex::decode(VALIDATOR2_PUBKEY) == registration.bridge_pubkey_bytes(),
            0,
        );

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
        committee.register(&mut system_state, hex::decode(VALIDATOR1_PUBKEY), b"", &tx(@0xA, 0));

        // Check committee before creation
        assert!(committee.members().is_empty());

        let ctx = test_scenario::ctx(&mut scenario);
        let voting_powers = sui_system::validator_voting_powers_for_testing(&mut system_state);
        try_create_next_committee(&mut committee, voting_powers, 6000, ctx);

        // committee should be empty because registration did not reach min stake threshold.
        assert!(committee.members().is_empty());

        test_utils::destroy(committee);
        test_scenario::return_shared(system_state);
        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code = bridge::committee::ECommitteeAlreadyInitiated)]
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
        committee.register(&mut system_state, hex::decode(VALIDATOR1_PUBKEY), b"", &tx(@0xA, 0));
        committee.register(&mut system_state, hex::decode(VALIDATOR2_PUBKEY), b"", &tx(@0xC, 0));
        assert!(committee.members().is_empty());
        let ctx = test_scenario::ctx(&mut scenario);
        let voting_powers = sui_system::validator_voting_powers_for_testing(&mut system_state);
        try_create_next_committee(&mut committee, voting_powers, 6000, ctx);

        test_scenario::next_tx(&mut scenario, @0x0);
        assert!(committee.members().size() == 2); // must succeed
        // this fails because committee is already initiated
        committee.register(&mut system_state, hex::decode(VALIDATOR1_PUBKEY), b"", &tx(@0xA, 0));

        abort 0
    }

    #[test]
    #[expected_failure(abort_code = bridge::committee::EInvalidPubkeyLength)]
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
        committee.register(&mut system_state, hex::decode(VALIDATOR2_PUBKEY), b"", &tx(@0xC, 0));
        // this fails with invalid public key
        committee.register(&mut system_state, b"029bef8", b"", &tx(@0xA, 0));

        abort 0
    }


    fun tx(sender: address, hint: u64): TxContext {
        tx_context::new_from_hint(sender, hint, 1, 0, 0)
    }

    #[test]
    #[expected_failure(abort_code = bridge::committee::ESignatureBelowThreshold)]
    fun test_verify_signatures_with_blocked_committee_member() {
        let mut committee = setup_test();
        let msg = message::deserialize_message_test_only(hex::decode(TEST_MSG));
        // good path, this test should have passed in previous test
        committee.verify_signatures(
            msg,
            vector[hex::decode(
                b"8ba030a450cb1e36f61e572645fc9da1dea5f79b6db663a21ab63286d7fc29af447433abdd0c0b35ab751154ac5b612ae64d3be810f0d9e10ff68e764514ced300"
            ), hex::decode(
                b"439379cc7b3ee3ebe1ff59d011dafc1caac47da6919b089c90f6a24e8c284b963b20f1f5421385456e57ac6b69c4b5f0d345aa09b8bc96d88d87051c7349e83801"
            )],
        );

        let (validator1, member) = committee.members().get_entry_by_idx(0);
        assert!(!member.blocklisted());

        // Block a member
        let blocklist = message::create_blocklist_message(
            chain_ids::sui_testnet(),
            0,
            0, // type 0 is block
            vector[crypto::ecdsa_pub_key_to_eth_address(validator1)]
        );
        let blocklist = message::extract_blocklist_payload(&blocklist);
        execute_blocklist(&mut committee, blocklist);

        let (_, blocked_member) = committee.members().get_entry_by_idx(0);
        assert!(blocked_member.blocklisted());

        // Verify signature should fail now
        committee.verify_signatures(
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
    #[expected_failure(abort_code = bridge::committee::EValidatorBlocklistContainsUnknownKey)]
    fun test_execute_blocklist_abort_upon_unknown_validator() {
        let mut committee = setup_test();

        // // val0 and val1 are not blocked yet
        let (validator0, _) = committee.members().get_entry_by_idx(0);
        // assert!(!member0.blocklisted());
        // let (validator1, member1) = committee.members().get_entry_by_idx(1);
        // assert!(!member1.blocklisted());

        let eth_address0 = crypto::ecdsa_pub_key_to_eth_address(validator0);
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
        let (validator0, member0) = committee.members().get_entry_by_idx(0);
        assert!(!member0.blocklisted());
        let (validator1, member1) = committee.members().get_entry_by_idx(1);
        assert!(!member1.blocklisted());

        let eth_address0 = crypto::ecdsa_pub_key_to_eth_address(validator0);
        let eth_address1 = crypto::ecdsa_pub_key_to_eth_address(validator1);

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
        let (_, blocked_member) = committee.members().get_entry_by_idx(0);
        assert!(blocked_member.blocklisted());
        // val 1 is too
        let (_, blocked_member) = committee.members().get_entry_by_idx(1);
        assert!(blocked_member.blocklisted());

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
        let (_, blocked_member) = committee.members().get_entry_by_idx(0);
        assert!(blocked_member.blocklisted());
        // val 1 is not
        let (_, blocked_member) = committee.members().get_entry_by_idx(1);
        assert!(!blocked_member.blocklisted());

        // Clean up
        test_utils::destroy(committee);
    }

    fun setup_test(): BridgeCommittee {
        let mut members = vec_map::empty<vector<u8>, CommitteeMember>();

        let bridge_pubkey_bytes = hex::decode(VALIDATOR1_PUBKEY);
        members.insert(
            bridge_pubkey_bytes,
            make_committee_member(
                @0xA,
                bridge_pubkey_bytes,
                3333,
                b"https://127.0.0.1:9191",
                false,
            ),
        );

        let bridge_pubkey_bytes = hex::decode(VALIDATOR2_PUBKEY);
        members.insert(
            bridge_pubkey_bytes,
            make_committee_member(
                @0xC,
                bridge_pubkey_bytes,
                3333,
                b"https://127.0.0.1:9192",
                false,
            ),
        );

        make_bridge_committee(members, vec_map::empty(), 1)
    }
}
