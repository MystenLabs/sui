// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module bridge::bridge_tests;
use bridge::bridge::{
    inner_limiter,
    inner_paused,
    inner_treasury,
    inner_token_transfer_records_mut,
    new_bridge_record_for_testing,
    new_for_testing,
    test_get_current_seq_num_and_increment,
    test_execute_update_asset_price,
    test_get_token_transfer_action_signatures,
    test_load_inner,
    test_load_inner_mut,
    test_get_token_transfer_action_status,
    transfer_status_approved,
    transfer_status_claimed,
    transfer_status_not_found,
    transfer_status_pending,
    Bridge
};
use bridge::bridge_env::{
    btc_id,
    create_bridge,
    create_bridge_default,
    create_env,
    create_validator,
    eth_id,
    freeze_bridge,
    init_committee,
    register_committee,
    unfreeze_bridge,
    test_token_id
};
use bridge::btc::BTC;
use bridge::chain_ids;
use bridge::eth::ETH;
use bridge::message::{Self, to_parsed_token_transfer_message};
use bridge::message_types;
use bridge::test_token::{TEST_TOKEN, create_bridge_token as create_test_token};
use bridge::usdc::USDC;
use std::type_name;
use sui::address;
use sui::balance;
use sui::coin::{Self, Coin};
use sui::hex;
use sui::package::test_publish;
use sui::test_scenario;
use sui::test_utils::destroy;

// common error start code for unexpected errors in tests (assertions).
// If more than one assert in a test needs to use an unexpected error code,
// use this as the starting error and add 1 to subsequent errors
const UNEXPECTED_ERROR: u64 = 10293847;
// use on tests that fail to save cleanup
const TEST_DONE: u64 = 74839201;

#[test]
fun test_bridge_create() {
    let mut env = create_env(chain_ids::sui_testnet());
    env.create_bridge(@0x0);

    let bridge = env.bridge(@0x0);
    let inner = bridge.bridge_ref().test_load_inner();
    inner.assert_not_paused(UNEXPECTED_ERROR);
    assert!(inner.inner_token_transfer_records().length() == 0);
    bridge.return_bridge();

    env.destroy_env();
}

#[test]
#[expected_failure(abort_code = bridge::bridge::ENotSystemAddress)]
fun test_bridge_create_non_system_addr() {
    let mut env = create_env(chain_ids::sui_mainnet());
    env.create_bridge(@0x1);

    abort TEST_DONE
}

#[test]
fun test_create_bridge_default() {
    let mut env = create_env(chain_ids::sui_custom());
    env.create_bridge_default();
    env.destroy_env();
}

#[test]
fun test_init_committee_twice() {
    let mut env = create_env(chain_ids::sui_testnet());
    env.create_bridge_default();
    env.init_committee(@0x0); // second time is a no-op

    env.destroy_env();
}

#[test]
#[expected_failure(abort_code = bridge::bridge::ENotSystemAddress)]
fun test_init_committee_non_system_addr() {
    let mut env = create_env(chain_ids::sui_mainnet());
    env.setup_validators(vector[
        create_validator(@0xA, 100, &b"12345678901234567890123456789012"),
    ]);
    env.create_bridge(@0x0);
    env.register_committee();
    env.init_committee(@0xA);

    abort TEST_DONE
}

#[test]
#[expected_failure(abort_code = bridge::committee::ECommitteeAlreadyInitiated)]
fun test_register_committee_after_init() {
    let mut env = create_env(chain_ids::sui_custom());
    env.create_bridge_default();
    env.register_committee();

    abort TEST_DONE
}

#[test]
fun test_register_foreign_token() {
    let addr = @0x0;
    let mut env = create_env(chain_ids::sui_testnet());
    env.create_bridge_default();
    let (upgrade_cap, treasury_cap, metadata) = create_test_token(env
        .scenario()
        .ctx());
    env.register_foreign_token<TEST_TOKEN>(
        treasury_cap,
        upgrade_cap,
        metadata,
        addr,
    );
    env.destroy_env();
}

#[test]
#[expected_failure(abort_code = bridge::treasury::ETokenSupplyNonZero)]
fun test_register_foreign_token_non_zero_supply() {
    let addr = @0x0;
    let mut env = create_env(chain_ids::sui_testnet());
    env.create_bridge_default();
    let (upgrade_cap, mut treasury_cap, metadata) = create_test_token(env
        .scenario()
        .ctx());
    let _coin = treasury_cap.mint(1, env.scenario().ctx());
    env.register_foreign_token<TEST_TOKEN>(
        treasury_cap,
        upgrade_cap,
        metadata,
        addr,
    );

    abort 0
}

#[test]
#[expected_failure(abort_code = bridge::treasury::EInvalidNotionalValue)]
fun test_add_token_price_zero_value() {
    let addr = @0x0;
    let mut env = create_env(chain_ids::sui_testnet());
    env.create_bridge_default();
    env.add_tokens(
        addr,
        false,
        vector[test_token_id()],
        vector[type_name::get<TEST_TOKEN>().into_string()],
        vector[0],
    );

    abort 0
}

#[test]
#[expected_failure(abort_code = bridge::bridge::EMalformedMessageError)]
fun test_add_token_malformed_1() {
    let addr = @0x0;
    let mut env = create_env(chain_ids::sui_testnet());
    env.create_bridge_default();
    env.add_tokens(
        addr,
        false,
        vector[test_token_id(), eth_id()],
        vector[type_name::get<TEST_TOKEN>().into_string()],
        vector[10],
    );

    abort 0
}

#[test]
#[expected_failure(abort_code = bridge::bridge::EMalformedMessageError)]
fun test_add_token_malformed_2() {
    let addr = @0x0;
    let mut env = create_env(chain_ids::sui_testnet());
    env.create_bridge_default();
    env.add_tokens(
        addr,
        false,
        vector[test_token_id()],
        vector[
            type_name::get<TEST_TOKEN>().into_string(),
            type_name::get<BTC>().into_string(),
        ],
        vector[10],
    );

    abort 0
}

#[test]
#[expected_failure(abort_code = bridge::bridge::EMalformedMessageError)]
fun test_add_token_malformed_3() {
    let addr = @0x0;
    let mut env = create_env(chain_ids::sui_testnet());
    env.create_bridge_default();
    env.add_tokens(
        addr,
        false,
        vector[test_token_id()],
        vector[type_name::get<TEST_TOKEN>().into_string()],
        vector[10, 20],
    );

    abort 0
}

#[test]
fun test_add_native_token_nop() {
    // adding a native token is simply a NO-OP at the moment
    let addr = @0x0;
    let mut env = create_env(chain_ids::sui_testnet());
    env.create_bridge_default();
    env.add_tokens(
        addr,
        true,
        vector[test_token_id()],
        vector[type_name::get<TEST_TOKEN>().into_string()],
        vector[100],
    );
    env.destroy_env();
}

#[test]
#[expected_failure(abort_code = bridge::treasury::EInvalidUpgradeCap)]
fun test_register_foreign_token_bad_upgrade_cap() {
    let addr = @0x0;
    let mut env = create_env(chain_ids::sui_testnet());
    env.create_bridge_default();
    let (_upgrade_cap, treasury_cap, metadata) = create_test_token(env
        .scenario()
        .ctx());
    let upgrade_cap = test_publish(@0x42.to_id(), env.scenario().ctx());
    env.register_foreign_token<TEST_TOKEN>(
        treasury_cap,
        upgrade_cap,
        metadata,
        addr,
    );

    abort 0
}

#[test]
fun test_execute_send_token() {
    let mut env = create_env(chain_ids::sui_testnet());
    env.create_bridge_default();
    let btc: Coin<BTC> = env.get_btc(1);
    let eth_address = x"0000000000000000000000000000000000000000";
    env.send_token(@0xABCD, chain_ids::eth_sepolia(), eth_address, btc);
    env.destroy_env();
}

#[test]
#[expected_failure(abort_code = bridge::bridge::ETokenValueIsZero)]
fun test_execute_send_token_zero_value() {
    let mut env = create_env(chain_ids::sui_testnet());
    env.create_bridge_default();
    let btc: Coin<BTC> = env.get_btc(0);
    let eth_address = x"0000000000000000000000000000000000000000";
    env.send_token(@0x0, chain_ids::eth_sepolia(), eth_address, btc);

    abort TEST_DONE
}

#[test]
#[expected_failure(abort_code = bridge::bridge::EInvalidEvmAddress)]
fun test_execute_send_token_invalid_evem_address() {
    let mut env = create_env(chain_ids::sui_testnet());
    env.create_bridge_default();
    let btc: Coin<BTC> = env.get_btc(1);
    let eth_address = x"1234";
    let val_addr = env.validators()[0].addr();
    env.send_token(val_addr, chain_ids::eth_sepolia(), eth_address, btc);

    abort TEST_DONE
}

#[test]
#[expected_failure(abort_code = bridge::bridge::EBridgeUnavailable)]
fun test_execute_send_token_frozen() {
    let chain_id = chain_ids::sui_testnet();
    let mut env = create_env(chain_id);
    env.create_bridge_default();
    let eth: Coin<ETH> = env.get_eth(1);
    let eth_address = x"0000000000000000000000000000000000000000";
    env.freeze_bridge(@0x0, UNEXPECTED_ERROR);
    env.send_token(@0xAAAA, chain_ids::eth_sepolia(), eth_address, eth);

    abort TEST_DONE
}

#[test]
#[expected_failure(abort_code = bridge::bridge::EInvalidBridgeRoute)]
fun test_execute_send_token_invalid_route() {
    let mut env = create_env(chain_ids::sui_testnet());
    env.create_bridge_default();
    let usdc: Coin<USDC> = env.get_usdc(100);
    let eth_address = x"0000000000000000000000000000000000000000";
    env.send_token(@0xABCDEF, chain_ids::eth_mainnet(), eth_address, usdc);

    abort TEST_DONE
}

#[test]
#[expected_failure(abort_code = bridge::bridge::EUnexpectedChainID)]
fun test_system_msg_incorrect_chain_id() {
    let sender = @0x0;
    let mut env = create_env(chain_ids::sui_testnet());
    env.create_bridge_default();
    env.execute_blocklist(sender, chain_ids::sui_mainnet(), 0, vector[]);

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
        ) ==
        0,
    );
    assert!(
        inner.sequence_nums()[&message_types::committee_blocklist()] == 1,
    );
    assert!(
        inner.test_get_current_seq_num_and_increment(
            message_types::committee_blocklist(),
        ) ==
        1,
    );
    // other message type nonce does not change
    assert!(
        !inner.sequence_nums().contains(&message_types::token()),
    );
    assert!(
        !inner.sequence_nums().contains(&message_types::emergency_op()),
    );
    assert!(
        !inner.sequence_nums().contains(&message_types::update_bridge_limit()),
    );
    assert!(
        !inner.sequence_nums().contains(&message_types::update_asset_price()),
    );
    assert!(
        inner.test_get_current_seq_num_and_increment(message_types::token()) ==
        0,
    );
    assert!(
        inner.test_get_current_seq_num_and_increment(
            message_types::emergency_op(),
        ) ==
        0,
    );
    assert!(
        inner.test_get_current_seq_num_and_increment(
            message_types::update_bridge_limit(),
        ) ==
        0,
    );
    assert!(
        inner.test_get_current_seq_num_and_increment(
            message_types::update_asset_price(),
        ) ==
        0,
    );

    destroy(bridge);
    scenario.end();
}

#[test]
fun test_update_limit() {
    let chain_id = chain_ids::sui_mainnet();
    let mut env = create_env(chain_id);
    env.create_bridge_default();

    let bridge = env.bridge(@0x0);
    let inner = bridge.bridge_ref().test_load_inner();
    // Assert the starting limit is a different value
    assert!(
        inner
            .inner_limiter()
            .get_route_limit(
                &chain_ids::get_route(
                    chain_ids::eth_mainnet(),
                    chain_ids::sui_mainnet(),
                ),
            ) !=
        1,
    );
    bridge.return_bridge();

    // update limit
    env.update_bridge_limit(
        @0x0,
        chain_ids::sui_mainnet(),
        chain_ids::eth_mainnet(),
        1,
    );

    let bridge = env.bridge(@0x0);
    let inner = bridge.bridge_ref().test_load_inner();
    // Assert the starting limit is a different value
    assert!(
        inner
            .inner_limiter()
            .get_route_limit(
                &chain_ids::get_route(
                    chain_ids::eth_mainnet(),
                    chain_ids::sui_mainnet(),
                ),
            ) ==
        1,
    );
    // other routes are not impacted
    assert!(
        inner
            .inner_limiter()
            .get_route_limit(
                &chain_ids::get_route(
                    chain_ids::eth_sepolia(),
                    chain_ids::sui_testnet(),
                ),
            ) !=
        1,
    );
    bridge.return_bridge();

    env.destroy_env();
}

#[test]
#[expected_failure(abort_code = bridge::bridge::EUnexpectedChainID)]
fun test_execute_update_bridge_limit_abort_with_unexpected_chain_id() {
    let mut env = create_env(chain_ids::sui_testnet());
    env.create_bridge_default();

    // This abort because the receiving_chain (sui_mainnet) is not the same as
    // the bridge's chain_id (sui_devnet)
    env.update_bridge_limit(
        @0x0,
        chain_ids::sui_mainnet(),
        chain_ids::eth_mainnet(),
        1,
    );

    abort TEST_DONE
}

#[test]
fun test_update_asset_price() {
    let mut env = create_env(chain_ids::sui_testnet());
    env.create_bridge_default();
    let scenario = env.scenario();
    scenario.next_tx(@0x0);
    let mut bridge = scenario.take_shared<Bridge>();
    let inner = bridge.test_load_inner_mut();

    // Assert the starting limit is a different value
    assert!(
        inner.inner_treasury().notional_value<BTC>() != 1_001_000_000,
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
    assert!(inner.inner_treasury().notional_value<BTC>() == 1_001_000_000);
    // other assets are not impacted
    assert!(inner.inner_treasury().notional_value<ETH>() != 1_001_000_000);

    destroy(bridge);
    env.destroy_env();
}

#[test]
#[expected_failure(abort_code = bridge::treasury::EInvalidNotionalValue)]
fun test_invalid_price_update() {
    let mut env = create_env(chain_ids::sui_testnet());
    env.create_bridge_default();
    env.update_asset_price(@0x0, btc_id(), 0);

    abort 0
}

#[test]
#[expected_failure(abort_code = bridge::treasury::EUnsupportedTokenType)]
fun test_unsupported_token_type() {
    let mut env = create_env(chain_ids::sui_testnet());
    env.create_bridge_default();
    env.update_asset_price(@0x0, 42, 100);

    abort 0
}

#[test]
fun test_execute_freeze_unfreeze() {
    let chain_id = chain_ids::sui_testnet();
    let mut env = create_env(chain_id);
    env.create_bridge_default();
    env.freeze_bridge(@0x0, UNEXPECTED_ERROR + 1);
    let bridge = env.bridge(@0x0);
    assert!(bridge.bridge_ref().test_load_inner().inner_paused());
    bridge.return_bridge();
    env.unfreeze_bridge(@0x0, UNEXPECTED_ERROR + 2);
    let bridge = env.bridge(@0x0);
    assert!(!bridge.bridge_ref().test_load_inner().inner_paused());
    bridge.return_bridge();
    env.destroy_env();
}

#[test]
#[expected_failure(abort_code = bridge::bridge::EBridgeNotPaused)]
fun test_execute_unfreeze_err() {
    let chain_id = chain_ids::sui_testnet();
    let mut env = create_env(chain_id);
    env.create_bridge_default();
    let bridge = env.bridge(@0x0);
    assert!(!bridge.bridge_ref().test_load_inner().inner_paused());
    bridge.return_bridge();
    env.unfreeze_bridge(@0x0, UNEXPECTED_ERROR + 2);

    abort TEST_DONE
}

#[test]
#[expected_failure(abort_code = bridge::bridge::EBridgeAlreadyPaused)]
fun test_execute_emergency_op_abort_when_already_frozen() {
    let chain_id = chain_ids::sui_testnet();
    let mut env = create_env(chain_id);
    env.create_bridge_default();

    // initially it's unfrozen
    let bridge = env.bridge(@0x0);
    assert!(!bridge.bridge_ref().test_load_inner().inner_paused());
    bridge.return_bridge();
    // freeze it
    env.freeze_bridge(@0x0, UNEXPECTED_ERROR);
    let bridge = env.bridge(@0x0);
    assert!(bridge.bridge_ref().test_load_inner().inner_paused());
    bridge.return_bridge();
    // freeze it again, should abort
    env.freeze_bridge(@0x0, UNEXPECTED_ERROR);

    abort TEST_DONE
}

#[test]
fun test_get_token_transfer_action_data() {
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
        hex::decode(
            b"00000000000000000000000000000000000000c8",
        ), // target_address
        1u8, // token_type
        coin.balance().value(),
    );

    let key = message.key();
    bridge
        .test_load_inner_mut()
        .inner_token_transfer_records_mut()
        .push_back(
            key,
            new_bridge_record_for_testing(message, option::none(), false),
        );
    assert!(
        bridge.test_get_token_transfer_action_status(chain_id, 10) ==
        transfer_status_pending(),
    );
    assert!(
        bridge.test_get_token_transfer_action_signatures(chain_id, 10) ==
        option::none(),
    );

    // Test when ready for claim
    let message = message::create_token_bridge_message(
        chain_ids::sui_testnet(), // source chain
        11, // seq_num
        address::to_bytes(ctx.sender()), // sender address
        chain_ids::eth_sepolia(), // target_chain
        hex::decode(
            b"00000000000000000000000000000000000000c8",
        ), // target_address
        1u8, // token_type
        balance::value(coin::balance(&coin)),
    );
    let key = message.key();
    bridge
        .test_load_inner_mut()
        .inner_token_transfer_records_mut()
        .push_back(
            key,
            new_bridge_record_for_testing(
                message,
                option::some(vector[]),
                false,
            ),
        );
    assert!(
        bridge.test_get_token_transfer_action_status(chain_id, 11) ==
        transfer_status_approved(),
    );
    assert!(
        bridge.test_get_token_transfer_action_signatures(chain_id, 11) ==
        option::some(vector[]),
    );
    assert!(
        bridge.test_get_parsed_token_transfer_message(chain_id, 11) ==
        option::some(
            to_parsed_token_transfer_message(&message),
        ),
    );

    // Test when already claimed
    let message = message::create_token_bridge_message(
        chain_ids::sui_testnet(), // source chain
        12, // seq_num
        address::to_bytes(ctx.sender()), // sender address
        chain_ids::eth_sepolia(), // target_chain
        hex::decode(
            b"00000000000000000000000000000000000000c8",
        ), // target_address
        1u8, // token_type
        balance::value(coin::balance(&coin)),
    );
    let key = message.key();
    bridge
        .test_load_inner_mut()
        .inner_token_transfer_records_mut()
        .push_back(
            key,
            new_bridge_record_for_testing(
                message,
                option::some(vector[b"1234"]),
                true,
            ),
        );
    assert!(
        bridge.test_get_token_transfer_action_status(chain_id, 12) ==
        transfer_status_claimed(),
    );
    assert!(
        bridge.test_get_token_transfer_action_signatures(chain_id, 12) ==
        option::some(vector[b"1234"]),
    );
    assert!(
        bridge.test_get_parsed_token_transfer_message(chain_id, 12) ==
        option::some(
            to_parsed_token_transfer_message(&message),
        ),
    );

    // Test when message not found
    assert!(
        bridge.test_get_token_transfer_action_status(chain_id, 13) ==
        transfer_status_not_found(),
    );
    assert!(
        bridge.test_get_token_transfer_action_signatures(chain_id, 13) ==
        option::none(),
    );
    assert!(
        bridge.test_get_parsed_token_transfer_message(chain_id, 13) ==
        option::none(),
    );

    destroy(bridge);
    coin.burn_for_testing();
    scenario.end();
}

#[test]
#[expected_failure(abort_code = bridge::treasury::EUnsupportedTokenType)]
fun test_get_metadata_no_token() {
    let chain_id = chain_ids::sui_testnet();
    let mut env = create_env(chain_id);
    env.create_bridge_default();
    let bridge = env.bridge(@0x0);
    let treasury = bridge.bridge_ref().test_load_inner().inner_treasury();
    treasury.notional_value<TEST_TOKEN>();

    abort 0
}

#[test]
fun change_url() {
    let chain_id = chain_ids::sui_testnet();
    let mut env = create_env(chain_id);
    env.create_bridge_default();
    let mut bridge = env.bridge(@0xAAAA);
    bridge
        .bridge_ref_mut()
        .update_node_url(b"<url_here>", env.scenario().ctx());
    bridge.return_bridge();
    env.destroy_env();
}

#[test]
#[
    expected_failure(
        abort_code = bridge::committee::ESenderIsNotInBridgeCommittee,
    ),
]
fun change_url_bad_sender() {
    let chain_id = chain_ids::sui_testnet();
    let mut env = create_env(chain_id);
    env.create_bridge_default();
    let mut bridge = env.bridge(@0x0);
    bridge
        .bridge_ref_mut()
        .update_node_url(b"<url_here>", env.scenario().ctx());
    abort 0
}
