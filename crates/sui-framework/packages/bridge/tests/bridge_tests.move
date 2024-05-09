// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module bridge::bridge_tests {
    use bridge::bridge::{
        assert_not_paused, assert_paused, create_bridge_for_testing, execute_system_message,
        get_token_transfer_action_status, inner_limiter, inner_paused,
        inner_treasury, inner_token_transfer_records, new_bridge_record_for_testing,
        new_for_testing, send_token, test_execute_emergency_op, test_init_bridge_committee,
        test_get_current_seq_num_and_increment, test_execute_update_asset_price,
        test_execute_update_bridge_limit, test_get_token_transfer_action_signatures,
        test_load_inner_mut, transfer_status_approved, transfer_status_claimed,
        transfer_status_not_found, transfer_status_pending,
        Bridge,
    };
    use bridge::chain_ids;
    use bridge::message::{Self, create_blocklist_message};
    use bridge::message_types;
    use bridge::treasury::{BTC, ETH};

    use sui::address;
    use sui::balance;
    use sui::coin::{Self, Coin};
    use sui::hex;
    use sui::test_scenario::{Self, Scenario};
    use sui::test_utils::destroy;

    use sui_system::{
        governance_test_utils::{
            advance_epoch_with_reward_amounts,
            create_sui_system_state_for_testing,
            create_validator_for_testing,
        },
        sui_system::{
            validator_voting_powers_for_testing,
            SuiSystemState,
        },
    };

    #[test_only]
    const VALIDATOR1_PUBKEY: vector<u8> = b"029bef8d556d80e43ae7e0becb3a7e6838b95defe45896ed6075bb9035d06c9964";
    #[test_only]
    const VALIDATOR2_PUBKEY: vector<u8> = b"033e99a541db69bd32040dfe5037fbf5210dafa8151a71e21c5204b05d95ce0a62";

    // common error start code for unexpected errors in tests (assertions).
    // If more than one assert in a test needs to use an unexpected error code,
    // use this as the starting error and add 1 to subsequent errors
    const UNEXPECTED_ERROR: u64 = 10293847;
    // use on tests that fail to save cleanup
    const TEST_DONE: u64 = 74839201;

    //
    // Utility functions
    //

    // Info to set up a validator
    public struct ValidatorInfo has copy, drop {
        validator: address,
        stake_amount: u64,
    }

    // Add a validator to the chain
    fun setup_validators(
        scenario: &mut Scenario,
        validators_info: vector<ValidatorInfo>,
        sender: address,
    ) {
        scenario.next_tx(sender);
        let ctx = scenario.ctx();
        let mut validators = vector::empty();
        let mut count = validators_info.length();
        while (count > 0) {
            count = count - 1;
            validators.push_back(create_validator_for_testing(
                validators_info[count].validator,
                validators_info[count].stake_amount,
                ctx,
            ));
        };
        create_sui_system_state_for_testing(validators, 0, 0, ctx);
        advance_epoch_with_reward_amounts(0, 0, scenario);
    }

    // Set up an environment for the bridge with a set of
    // validators, a bridge with a treasury and a committee.
    // Save the Bridge as a shared object.
    fun create_bridge_default(scenario: &mut Scenario) {
        let sender = @0x0;

        let validators = vector[
            ValidatorInfo { validator: @0xA, stake_amount: 100 },
            ValidatorInfo { validator: @0xB, stake_amount: 100 },
            ValidatorInfo { validator: @0xC, stake_amount: 100 },
        ];
        setup_validators(scenario, validators, sender);

        create_bridge(scenario, sender);
    }

    // Create a bridge and set up a treasury
    fun create_bridge(scenario: &mut Scenario, sender: address) {
        scenario.next_tx(sender);
        let ctx = scenario.ctx();
        let chain_id = chain_ids::sui_testnet();
        create_bridge_for_testing(object::new(ctx), chain_id, ctx);

        scenario.next_tx(sender);
        let mut bridge = scenario.take_shared<Bridge>();
        bridge.setup_treasury_for_testing();

        test_scenario::return_shared(bridge);
    }

    // Register two committee members
    fun register_committee(scenario: &mut Scenario) {
        scenario.next_tx(@0x0);
        let mut bridge = scenario.take_shared<Bridge>();
        let mut system_state = test_scenario::take_shared<SuiSystemState>(scenario);

        // register committee member `0xA`
        scenario.next_tx(@0xA);
        bridge.committee_registration(
            &mut system_state,
            hex::decode(VALIDATOR1_PUBKEY),
            b"",
            scenario.ctx(),
        );

        // register committee member `0xC`
        scenario.next_tx(@0xC);
        bridge.committee_registration(
            &mut system_state,
            hex::decode(VALIDATOR2_PUBKEY),
            b"",
            scenario.ctx(),
        );

        test_scenario::return_shared(bridge);
        test_scenario::return_shared(system_state);
    }

    // Init the bridge committee
    fun init_committee(scenario: &mut Scenario, sender: address) {
        // init committee
        scenario.next_tx(sender);
        let mut bridge = scenario.take_shared<Bridge>();
        let mut system_state = test_scenario::take_shared<SuiSystemState>(scenario);
        let voting_powers = validator_voting_powers_for_testing(&mut system_state);
        bridge.test_init_bridge_committee(
            voting_powers,
            50,
            scenario.ctx(),
        );
        test_scenario::return_shared(bridge);
        test_scenario::return_shared(system_state);
    }

    // Freeze the bridge
    fun freeze_bridge(bridge: &mut Bridge, error: u64) {
        let inner = bridge.test_load_inner_mut();
        // freeze it
        let msg = message::create_emergency_op_message(
            chain_ids::sui_testnet(),
            0, // seq num
            0, // freeze op
        );
        let payload = msg.extract_emergency_op_payload();
        inner.test_execute_emergency_op(payload);
        inner.assert_paused(error);
    }

    // unfreeze the bridge
    fun unfreeze_bridge(bridge: &mut Bridge, error: u64) {
        let inner = bridge.test_load_inner_mut();
        // unfreeze it
        let msg = message::create_emergency_op_message(
            chain_ids::sui_testnet(),
            1, // seq num, this is supposed to be the next seq num but it's not what we test here
            1, // unfreeze op
        );
        let payload = msg.extract_emergency_op_payload();
        inner.test_execute_emergency_op(payload);
        inner.assert_not_paused(error);
    }

    #[test]
    fun test_bridge_create() {
        let mut scenario = test_scenario::begin(@0x0);
        create_bridge_default(&mut scenario);

        scenario.next_tx(@0xAAAA);
        let mut bridge = scenario.take_shared<Bridge>();
        let inner = bridge.test_load_inner_mut();
        inner.assert_not_paused(UNEXPECTED_ERROR);
        assert!(inner.inner_token_transfer_records().length() == 0, UNEXPECTED_ERROR + 1);

        test_scenario::return_shared(bridge);
        scenario.end();
    }

    #[test]
    #[expected_failure(abort_code = bridge::bridge::ENotSystemAddress)]
    fun test_bridge_create_non_system_addr() {
        let mut scenario = test_scenario::begin(@0x1);
        create_bridge(&mut scenario, @0x1);

        abort TEST_DONE
    }

    #[test]
    fun test_init_committee() {
        let mut scenario = test_scenario::begin(@0x0);

        create_bridge_default(&mut scenario);
        register_committee(&mut scenario);
        init_committee(&mut scenario, @0x0);

        scenario.end();
    }

    #[test]
    fun test_init_committee_twice() {
        let mut scenario = test_scenario::begin(@0x0);

        create_bridge_default(&mut scenario);
        register_committee(&mut scenario);
        init_committee(&mut scenario, @0x0);
        init_committee(&mut scenario, @0x0); // second time is a no-op

        scenario.end();
    }

    #[test]
    #[expected_failure(abort_code = bridge::bridge::ENotSystemAddress)]
    fun test_init_committee_non_system_addr() {
        let mut scenario = test_scenario::begin(@0x0);

        create_bridge_default(&mut scenario);
        register_committee(&mut scenario);
        init_committee(&mut scenario, @0xA);


        abort TEST_DONE
    }

    #[test]
    #[expected_failure(abort_code = bridge::committee::ECommitteeAlreadyInitiated)]
    fun test_register_committee_after_init() {
        let mut scenario = test_scenario::begin(@0x0);

        create_bridge_default(&mut scenario);
        register_committee(&mut scenario);
        init_committee(&mut scenario, @0x0);
        register_committee(&mut scenario);


        abort TEST_DONE
    }

    // #[test]
    // fun test_register_foreign_token() {
    //     let mut scenario = test_scenario::begin(@0x0);

    //     create_bridge_default(&mut scenario);
    //     register_committee(&mut scenario);
    //     init_committee(&mut scenario, @0x0);

    //     scenario.next_tx(@0xAAAA);
    //     let mut bridge = scenario.take_shared<Bridge>();
    //     bridge.register_foreign_token<T>(
    //         tc: TreasuryCap<T>,
    //         uc: UpgradeCap,
    //         metadata: &CoinMetadata<T>,
    //     );

    //     scenario.end();
    // }

    // #[test]
    // fun test_execute_send_token() {
    //     let mut scenario = test_scenario::begin(@0x0);

    //     create_bridge_default(&mut scenario);
    //     register_committee(&mut scenario);
    //     init_committee(&mut scenario, @0x0);

    //     scenario.next_tx(@0xAAAA);
    //     let mut bridge = scenario.take_shared<Bridge>();
    //     let eth_address = b"01234"; // it does not really matter
    //     let btc: Coin<BTC> = coin::mint_for_testing<BTC>(1, scenario.ctx());
    //     bridge.send_token(
    //         chain_ids::eth_sepolia(),
    //         eth_address,
    //         btc,
    //         scenario.ctx(),
    //     );
    //     test_scenario::return_shared(bridge);

    //     scenario.end();
    // }

    #[test]
    #[expected_failure(abort_code = bridge::bridge::EBridgeUnavailable)]
    fun test_execute_send_token_frozen() {
        let mut scenario = test_scenario::begin(@0x0);
        let ctx = scenario.ctx();
        let chain_id = chain_ids::sui_testnet();
        let mut bridge = new_for_testing(chain_id, ctx);

        assert!(!bridge.test_load_inner_mut().inner_paused(), UNEXPECTED_ERROR);
        freeze_bridge(&mut bridge, UNEXPECTED_ERROR + 1);

        let eth_address = b"01234"; // it does not really matter
        let btc: Coin<BTC> = coin::mint_for_testing<BTC>(1, ctx);
        bridge.send_token(
            chain_ids::eth_sepolia(),
            eth_address,
            btc,
            ctx,
        );

        abort TEST_DONE
    }

    #[test]
    #[expected_failure(abort_code = bridge::bridge::EInvalidBridgeRoute)]
    fun test_execute_send_token_invalid_route() {
        let mut scenario = test_scenario::begin(@0x0);
        let ctx = scenario.ctx();
        let chain_id = chain_ids::sui_testnet();
        let mut bridge = new_for_testing(chain_id, ctx);

        let eth_address = b"01234"; // it does not really matter
        let btc: Coin<BTC> = coin::mint_for_testing<BTC>(1, ctx);
        bridge.send_token(
            chain_ids::eth_mainnet(),
            eth_address,
            btc,
            ctx,
        );

        abort TEST_DONE
    }

    #[test]
    #[expected_failure(abort_code = bridge::bridge::EUnexpectedChainID)]
    fun test_system_msg_incorrect_chain_id() {
        let mut scenario = test_scenario::begin(@0x0);
        let ctx = scenario.ctx();
        let chain_id = chain_ids::sui_testnet();
        let mut bridge = new_for_testing(chain_id, ctx);
        let blocklist = create_blocklist_message(chain_ids::sui_mainnet(), 0, 0, vector[]);
        bridge.execute_system_message(blocklist, vector[]);

        abort TEST_DONE
    }

    #[test]
    fun test_get_seq_num_and_increment() {
        let mut scenario = test_scenario::begin(@0x0);
        let ctx = scenario.ctx();
        let chain_id = chain_ids::sui_testnet();
        let mut bridge = new_for_testing(chain_id, ctx);

        let inner = bridge.test_load_inner_mut();
        assert!(
            inner.test_get_current_seq_num_and_increment(
                message_types::committee_blocklist(),
            ) == 0,
            UNEXPECTED_ERROR,
        );
        assert!(
            inner.sequence_nums()[&message_types::committee_blocklist()] == 1,
            UNEXPECTED_ERROR + 1,
        );
        assert!(
            inner.test_get_current_seq_num_and_increment(
                message_types::committee_blocklist(),
            ) == 1,
            UNEXPECTED_ERROR + 2,
        );
        // other message type nonce does not change
        assert!(
            !inner.sequence_nums().contains(&message_types::token()),
            UNEXPECTED_ERROR + 3,
        );
        assert!(
            !inner.sequence_nums().contains(&message_types::emergency_op()),
            UNEXPECTED_ERROR + 4,
        );
        assert!(
            !inner.sequence_nums().contains(&message_types::update_bridge_limit()),
            UNEXPECTED_ERROR + 5,
        );
        assert!(
            !inner.sequence_nums().contains(&message_types::update_asset_price()),
            UNEXPECTED_ERROR + 6,
        );
        assert!(
            inner.test_get_current_seq_num_and_increment(message_types::token()) == 0,
            UNEXPECTED_ERROR + 7,
        );
        assert!(
            inner.test_get_current_seq_num_and_increment(
                message_types::emergency_op(),
            ) == 0,
            UNEXPECTED_ERROR + 8,
        );
        assert!(
            inner.test_get_current_seq_num_and_increment(
                message_types::update_bridge_limit(),
            ) == 0,
            UNEXPECTED_ERROR + 6,
        );
        assert!(
            inner.test_get_current_seq_num_and_increment(
                message_types::update_asset_price(),
            ) == 0,
            UNEXPECTED_ERROR + 7,
        );

        destroy(bridge);
        scenario.end();
    }

    #[test]
    fun test_update_limit() {
        let mut scenario = test_scenario::begin(@0x0);
        let ctx = scenario.ctx();
        let chain_id = chain_ids::sui_mainnet();
        let mut bridge = new_for_testing(chain_id, ctx);
        let inner = bridge.test_load_inner_mut();

        // Assert the starting limit is a different value
        assert!(
            inner.inner_limiter().get_route_limit(
                &chain_ids::get_route(
                    chain_ids::eth_mainnet(),
                    chain_ids::sui_mainnet(),
                ),
            ) != 1,
            UNEXPECTED_ERROR,
        );
        // now shrink to 1 for SUI mainnet -> ETH mainnet
        let msg = message::create_update_bridge_limit_message(
            chain_ids::sui_mainnet(), // receiving_chain
            0,
            chain_ids::eth_mainnet(), // sending_chain
            1,
        );
        let payload = msg.extract_update_bridge_limit();
        inner.test_execute_update_bridge_limit(payload);

        // should be 1 now
        assert!(
            inner.inner_limiter().get_route_limit(
                &chain_ids::get_route(
                    chain_ids::eth_mainnet(),
                    chain_ids::sui_mainnet()
                ),
            ) == 1,
            UNEXPECTED_ERROR + 1,
        );
        // other routes are not impacted
        assert!(
            inner.inner_limiter().get_route_limit(
                &chain_ids::get_route(
                    chain_ids::eth_sepolia(),
                    chain_ids::sui_testnet(),
                ),
            ) != 1,
            UNEXPECTED_ERROR + 2,
        );

        destroy(bridge);
        scenario.end();
    }

    #[test]
    #[expected_failure(abort_code = bridge::bridge::EUnexpectedChainID)]
    fun test_execute_update_bridge_limit_abort_with_unexpected_chain_id() {
        let mut scenario = test_scenario::begin(@0x0);
        let ctx = scenario.ctx();
        let chain_id = chain_ids::sui_testnet();
        let mut bridge = new_for_testing(chain_id, ctx);
        let inner = bridge.test_load_inner_mut();

        // shrink to 1 for SUI mainnet -> ETH mainnet
        let msg = message::create_update_bridge_limit_message(
            chain_ids::sui_mainnet(), // receiving_chain
            0,
            chain_ids::eth_mainnet(), // sending_chain
            1,
        );
        let payload = msg.extract_update_bridge_limit();
        // This abort because the receiving_chain (sui_mainnet) is not the same as
        // the bridge's chain_id (sui_devnet)
        inner.test_execute_update_bridge_limit(payload);

        abort TEST_DONE
    }


    #[test]
    fun test_update_asset_price() {
        let mut scenario = test_scenario::begin(@0x0);
        let ctx = scenario.ctx();
        let chain_id = chain_ids::sui_testnet();
        let mut bridge = new_for_testing(chain_id, ctx);
        let inner = bridge.test_load_inner_mut();

        // Assert the starting limit is a different value
        assert!(
            inner.inner_treasury().notional_value<BTC>() != 1_001_000_000,
            UNEXPECTED_ERROR,
        );
        // now change it to 100_001_000
        let msg = message::create_update_asset_price_message(
            inner.inner_treasury().token_id<BTC>(),
            chain_ids::sui_mainnet(),
            0,
            1_001_000_000,
        );
        let payload = msg.extract_update_asset_price();
        inner.test_execute_update_asset_price(payload);

        // should be 1_001_000_000 now
        assert!(
            inner.inner_treasury().notional_value<BTC>() == 1_001_000_000,
            UNEXPECTED_ERROR + 1,
        );
        // other assets are not impacted
        assert!(
            inner.inner_treasury().notional_value<ETH>() != 1_001_000_000,
            UNEXPECTED_ERROR + 2,
        );

        destroy(bridge);
        scenario.end();
    }

    #[test]
    fun test_test_execute_emergency_op() {
        let mut scenario = test_scenario::begin(@0x0);
        let ctx = scenario.ctx();
        let chain_id = chain_ids::sui_testnet();
        let mut bridge = new_for_testing(chain_id, ctx);

        assert!(!bridge.test_load_inner_mut().inner_paused(), UNEXPECTED_ERROR);
        freeze_bridge(&mut bridge, UNEXPECTED_ERROR + 1);

        assert!(bridge.test_load_inner_mut().inner_paused(), UNEXPECTED_ERROR + 2);
        unfreeze_bridge(&mut bridge, UNEXPECTED_ERROR + 3);

        destroy(bridge);
        scenario.end();
    }

    #[test]
    #[expected_failure(abort_code = bridge::bridge::EBridgeNotPaused)]
    fun test_test_execute_emergency_op_abort_when_not_frozen() {
        let mut scenario = test_scenario::begin(@0x0);
        let ctx = scenario.ctx();
        let chain_id = chain_ids::sui_testnet();
        let mut bridge = new_for_testing(chain_id, ctx);

        assert!(!bridge.test_load_inner_mut().inner_paused(), UNEXPECTED_ERROR);
        // unfreeze it, should abort
        unfreeze_bridge(&mut bridge, UNEXPECTED_ERROR + 1);

        abort TEST_DONE
    }

    #[test]
    #[expected_failure(abort_code = bridge::bridge::EBridgeAlreadyPaused)]
    fun test_test_execute_emergency_op_abort_when_already_frozen() {
        let mut scenario = test_scenario::begin(@0x0);
        let ctx = scenario.ctx();
        let chain_id = chain_ids::sui_testnet();
        let mut bridge = new_for_testing(chain_id, ctx);
        let inner = bridge.test_load_inner_mut();

        // initially it's unfrozen
        assert!(!inner.inner_paused(), UNEXPECTED_ERROR);
        // freeze it
        let msg = message::create_emergency_op_message(
            chain_ids::sui_testnet(),
            0, // seq num
            0, // freeze op
        );
        let payload = msg.extract_emergency_op_payload();
        inner.test_execute_emergency_op(payload);

        // should be frozen now
        assert!(inner.inner_paused(), UNEXPECTED_ERROR + 1);

        // freeze it again, should abort
        let msg = message::create_emergency_op_message(
            chain_ids::sui_testnet(),
            1, // seq num, should be the next seq num but it's not what we test here
            0, // unfreeze op
        );
        let payload = msg.extract_emergency_op_payload();
        inner.test_execute_emergency_op(payload);

        abort TEST_DONE
    }

    #[test]
    fun test_get_token_transfer_action_status() {
        let mut scenario = test_scenario::begin(@0x0);
        let ctx = scenario.ctx();
        let chain_id = chain_ids::sui_testnet();
        let mut bridge = new_for_testing(chain_id, ctx);
        let coin = coin::mint_for_testing<ETH>(12345, ctx);

        // Test when pending
        let message = message::create_token_bridge_message(
            chain_ids::sui_testnet(), // source chain
            10, // seq_num
            address::to_bytes(ctx.sender()), // sender address
            chain_ids::eth_sepolia(), // target_chain
            hex::decode(b"00000000000000000000000000000000000000c8"), // target_address
            1u8, // token_type
            coin.balance().value(),
        );

        let key = message.key();
        bridge.test_load_inner_mut().inner_token_transfer_records().push_back(
            key,
            new_bridge_record_for_testing(message, option::none(), false),
        );
        assert!(
            bridge.get_token_transfer_action_status(chain_id, 10)
                == transfer_status_pending(),
            UNEXPECTED_ERROR,
        );
        assert!(
            bridge.test_get_token_transfer_action_signatures(chain_id, 10) == option::none(),
            UNEXPECTED_ERROR + 1,
        );

        // Test when ready for claim
        let message = message::create_token_bridge_message(
            chain_ids::sui_testnet(), // source chain
            11, // seq_num
            address::to_bytes(ctx.sender()), // sender address
            chain_ids::eth_sepolia(), // target_chain
            hex::decode(b"00000000000000000000000000000000000000c8"), // target_address
            1u8, // token_type
            balance::value(coin::balance(&coin))
        );
        let key = message.key();
        bridge.test_load_inner_mut().inner_token_transfer_records().push_back(
            key,
            new_bridge_record_for_testing(message, option::some(vector[]), false),
        );
        assert!(
            bridge.get_token_transfer_action_status(chain_id, 11)
                == transfer_status_approved(),
            UNEXPECTED_ERROR + 2,
        );
        assert!(
            bridge.test_get_token_transfer_action_signatures(chain_id, 11)
                == option::some(vector[]),
            UNEXPECTED_ERROR + 3,
        );

        // Test when already claimed
        let message = message::create_token_bridge_message(
            chain_ids::sui_testnet(), // source chain
            12, // seq_num
            address::to_bytes(ctx.sender()), // sender address
            chain_ids::eth_sepolia(), // target_chain
            hex::decode(b"00000000000000000000000000000000000000c8"), // target_address
            1u8, // token_type
            balance::value(coin::balance(&coin))
        );
        let key = message.key();
        bridge.test_load_inner_mut().inner_token_transfer_records().push_back(
            key,
            new_bridge_record_for_testing(message, option::some(vector[b"1234"]), true),
        );
        assert!(
            bridge.get_token_transfer_action_status(chain_id, 12)
                == transfer_status_claimed(),
            UNEXPECTED_ERROR + 3,
        );
        assert!(
            bridge.test_get_token_transfer_action_signatures(chain_id, 12)
                == option::some(vector[b"1234"]),
            UNEXPECTED_ERROR + 4,
        );

        // Test when message not found
        assert!(
            bridge.get_token_transfer_action_status(chain_id, 13)
                == transfer_status_not_found(),
            UNEXPECTED_ERROR + 5,
        );
        assert!(
            bridge.test_get_token_transfer_action_signatures(chain_id, 13)
                == option::none(),
            UNEXPECTED_ERROR + 6,
        );

        destroy(bridge);
        coin.burn_for_testing();
        scenario.end();
    }
}

