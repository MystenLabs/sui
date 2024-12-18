// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module bridge::bridge_env {
    use bridge::bridge::{
        assert_not_paused,
        assert_paused,
        create_bridge_for_testing,
        inner_token_transfer_records,
        test_init_bridge_committee,
        test_load_inner_mut,
        Bridge,
        EmergencyOpEvent,
        TokenDepositedEvent,
        TokenTransferAlreadyApproved,
        TokenTransferAlreadyClaimed,
        TokenTransferApproved,
        TokenTransferClaimed,
        TokenTransferLimitExceed
    };
    use bridge::btc::{Self, BTC};
    use bridge::chain_ids;
    use bridge::committee::BlocklistValidatorEvent;
    use bridge::eth::{Self, ETH};
    use bridge::limiter::UpdateRouteLimitEvent;
    use bridge::message::{
        Self,
        BridgeMessage,
        create_add_tokens_on_sui_message,
        create_blocklist_message,
        emergency_op_pause,
        emergency_op_unpause
    };
    use bridge::message_types;
    use bridge::test_token::{Self, TEST_TOKEN};
    use bridge::treasury::{
        TokenRegistrationEvent,
        NewTokenEvent,
        UpdateTokenPriceEvent
    };
    use bridge::usdc::{Self, USDC};
    use bridge::usdt::{Self, USDT};
    use std::ascii::String;
    use std::type_name;
    use sui::address;
    use sui::clock::Clock;
    use sui::coin::{Self, Coin, CoinMetadata, TreasuryCap};
    use sui::ecdsa_k1::{KeyPair, secp256k1_keypair_from_seed, secp256k1_sign};
    use sui::event;
    use sui::package::UpgradeCap;
    use sui::test_scenario::{Self, Scenario};
    use sui::test_utils::destroy;
    use sui_system::governance_test_utils::{
        advance_epoch_with_reward_amounts,
        create_sui_system_state_for_testing,
        create_validator_for_testing
    };
    use sui_system::sui_system::{
        validator_voting_powers_for_testing,
        SuiSystemState
    };

    //
    // Token IDs
    //
    const BTC_ID: u8 = 1;
    const ETH_ID: u8 = 2;
    const USDC_ID: u8 = 3;
    const USDT_ID: u8 = 4;
    const TEST_TOKEN_ID: u8 = 5;

    public fun btc_id(): u8 {
        BTC_ID
    }

    public fun eth_id(): u8 {
        ETH_ID
    }

    public fun usdc_id(): u8 {
        USDC_ID
    }

    public fun usdt_id(): u8 {
        USDT_ID
    }

    public fun test_token_id(): u8 {
        TEST_TOKEN_ID
    }

    //
    // Claim status
    //
    const CLAIMED: u8 = 1;
    const ALREADY_CLAIMED: u8 = 2;
    const LIMIT_EXCEEDED: u8 = 3;

    public fun claimed(): u8 {
        CLAIMED
    }

    public fun already_claimed(): u8 {
        ALREADY_CLAIMED
    }

    public fun limit_exceeded(): u8 {
        LIMIT_EXCEEDED
    }

    //
    // Approve status
    //
    const APPROVED: u8 = 1;
    const ALREADY_APPROVED: u8 = 2;

    public fun approved(): u8 {
        APPROVED
    }

    public fun already_approved(): u8 {
        ALREADY_APPROVED
    }

    //
    // Validators setup and info
    //

    // Validator info
    public struct ValidatorInfo has drop {
        validator: address,
        key_pair: KeyPair,
        stake_amount: u64,
    }

    public fun addr(validator: &ValidatorInfo): address {
        validator.validator
    }

    public fun public_key(validator: &ValidatorInfo): &vector<u8> {
        validator.key_pair.public_key()
    }

    public fun create_validator(
        validator: address,
        stake_amount: u64,
        seed: &vector<u8>,
    ): ValidatorInfo {
        ValidatorInfo {
            validator,
            key_pair: secp256k1_keypair_from_seed(seed),
            stake_amount,
        }
    }

    // Bridge environemnt
    public struct BridgeEnv {
        scenario: Scenario,
        validators: vector<ValidatorInfo>,
        chain_id: u8,
        vault: Vault,
        clock: Clock,
    }

    // Holds coins for different bridged tokens
    public struct Vault {
        btc_coins: Coin<BTC>,
        eth_coins: Coin<ETH>,
        usdc_coins: Coin<USDC>,
        usdt_coins: Coin<USDT>,
        test_coins: Coin<TEST_TOKEN>,
    }

    // HotPotato to access shared state
    // TODO: if the bridge is the only shared state we could remvove this
    public struct BridgeWrapper {
        bridge: Bridge,
    }

    public fun bridge(env: &mut BridgeEnv, sender: address): BridgeWrapper {
        let scenario = &mut env.scenario;
        scenario.next_tx(sender);
        let bridge = scenario.take_shared<Bridge>();
        BridgeWrapper { bridge }
    }

    public fun bridge_ref(wrapper: &BridgeWrapper): &Bridge {
        &wrapper.bridge
    }

    public fun bridge_ref_mut(wrapper: &mut BridgeWrapper): &mut Bridge {
        &mut wrapper.bridge
    }

    public fun return_bridge(bridge: BridgeWrapper) {
        let BridgeWrapper { bridge } = bridge;
        test_scenario::return_shared(bridge);
    }

    //
    // Public functions
    //

    //
    // Environment creation and destruction
    //

    public fun create_env(chain_id: u8): BridgeEnv {
        let mut scenario = test_scenario::begin(@0x0);
        let ctx = scenario.ctx();
        let mut clock = sui::clock::create_for_testing(ctx);
        clock.set_for_testing(1_000_000_000);
        let btc_coins = coin::zero<BTC>(ctx);
        let eth_coins = coin::zero<ETH>(ctx);
        let usdc_coins = coin::zero<USDC>(ctx);
        let usdt_coins = coin::zero<USDT>(ctx);
        let test_coins = coin::zero<TEST_TOKEN>(ctx);
        let vault = Vault {
            btc_coins,
            eth_coins,
            usdc_coins,
            usdt_coins,
            test_coins,
        };
        BridgeEnv {
            scenario,
            chain_id,
            vault,
            validators: vector::empty(),
            clock,
        }
    }

    public fun destroy_env(env: BridgeEnv) {
        let BridgeEnv {
            scenario,
            chain_id: _,
            vault,
            validators: _,
            clock,
        } = env;
        destroy_valut(vault);
        clock.destroy_for_testing();
        scenario.end();
    }

    //
    // Add a set of validators to the chain.
    // Call only once in a test scenario.
    public fun setup_validators(
        env: &mut BridgeEnv,
        validators_info: vector<ValidatorInfo>,
    ) {
        let scenario = &mut env.scenario;
        scenario.next_tx(@0x0);
        let ctx = scenario.ctx();
        let validators = validators_info.map_ref!(
            |validator| {
                create_validator_for_testing(
                    validator.validator,
                    validator.stake_amount,
                    ctx,
                )
            },
        );
        env.validators = validators_info;
        create_sui_system_state_for_testing(validators, 0, 0, ctx);
        advance_epoch_with_reward_amounts(0, 0, scenario);
    }

    //
    // Bridge creation and setup
    //

    // Set up an environment with 3 validators, a bridge with
    // a treasury and a committee with all 3 validators.
    // The treasury will contain 4 tokens: ETH, BTC, USDT, USDC.
    // Save the Bridge as a shared object.
    public fun create_bridge_default(env: &mut BridgeEnv) {
        let validators = vector[
            create_validator(
                @0xAAAA,
                100,
                &b"1234567890_1234567890_1234567890",
            ),
            create_validator(
                @0xBBBB,
                100,
                &b"234567890_1234567890_1234567890_",
            ),
            create_validator(
                @0xCCCC,
                100,
                &b"34567890_1234567890_1234567890_1",
            ),
        ];
        env.setup_validators(validators);

        let sender = @0x0;
        env.create_bridge(sender);
        env.register_committee();
        env.init_committee(sender);
        env.setup_treasury(sender);
    }

    // Create a bridge and set up a treasury.
    // The treasury will contain 4 tokens: ETH, BTC, USDT, USDC.
    // Save the Bridge as a shared object.
    // No operation on the validators.
    public fun create_bridge(env: &mut BridgeEnv, sender: address) {
        env.scenario.next_tx(sender);
        let ctx = env.scenario.ctx();
        create_bridge_for_testing(object::new(ctx), env.chain_id, ctx);
    }

    // Register 3 committee members (validators `@0xA`, `@0xB`, `@0xC`)
    public fun register_committee(env: &mut BridgeEnv) {
        let scenario = &mut env.scenario;
        scenario.next_tx(@0x0);
        let mut bridge = scenario.take_shared<Bridge>();
        let mut system_state = test_scenario::take_shared<SuiSystemState>(
            scenario,
        );

        env
            .validators
            .do_ref!(
                |validator| {
                    scenario.next_tx(validator.validator);
                    bridge.committee_registration(
                        &mut system_state,
                        *validator.key_pair.public_key(),
                        b"",
                        scenario.ctx(),
                    );
                },
            );

        test_scenario::return_shared(bridge);
        test_scenario::return_shared(system_state);
    }

    // Init the bridge committee
    public fun init_committee(env: &mut BridgeEnv, sender: address) {
        let scenario = &mut env.scenario;
        scenario.next_tx(sender);
        let mut bridge = scenario.take_shared<Bridge>();
        let mut system_state = test_scenario::take_shared<SuiSystemState>(
            scenario,
        );
        let voting_powers = validator_voting_powers_for_testing(
            &mut system_state,
        );
        bridge.test_init_bridge_committee(
            voting_powers,
            50,
            scenario.ctx(),
        );
        test_scenario::return_shared(bridge);
        test_scenario::return_shared(system_state);
    }

    // Set up a treasury with 4 tokens: ETH, BTC, USDT, USDC.
    public fun setup_treasury(env: &mut BridgeEnv, sender: address) {
        env.register_default_tokens(sender);
        env.add_default_tokens(sender);
        env.load_vault(sender);
    }

    // Register 4 tokens with the Bridge: ETH, BTC, USDT, USDC.
    fun register_default_tokens(env: &mut BridgeEnv, sender: address) {
        env.scenario.next_tx(sender);
        let mut bridge = env.scenario.take_shared<Bridge>();

        // BTC
        let (upgrade_cap, treasury_cap, metadata) = btc::create_bridge_token(env
            .scenario
            .ctx());
        bridge.register_foreign_token<BTC>(
            treasury_cap,
            upgrade_cap,
            &metadata,
        );
        destroy(metadata);
        // ETH
        let (upgrade_cap, treasury_cap, metadata) = eth::create_bridge_token(env
            .scenario
            .ctx());
        bridge.register_foreign_token<ETH>(
            treasury_cap,
            upgrade_cap,
            &metadata,
        );
        destroy(metadata);
        // USDC
        let (
            upgrade_cap,
            treasury_cap,
            metadata,
        ) = usdc::create_bridge_token(env.scenario.ctx());
        bridge.register_foreign_token<USDC>(
            treasury_cap,
            upgrade_cap,
            &metadata,
        );
        destroy(metadata);
        // USDT
        let (
            upgrade_cap,
            treasury_cap,
            metadata,
        ) = usdt::create_bridge_token(env.scenario.ctx());
        bridge.register_foreign_token<USDT>(
            treasury_cap,
            upgrade_cap,
            &metadata,
        );
        destroy(metadata);

        test_scenario::return_shared(bridge);
    }

    // Add the 4 tokens previously registered: ETH, BTC, USDT, USDC.
    fun add_default_tokens(env: &mut BridgeEnv, sender: address) {
        let scenario = &mut env.scenario;
        scenario.next_tx(sender);
        let mut bridge = scenario.take_shared<Bridge>();

        let add_token_message = create_add_tokens_on_sui_message(
            env.chain_id,
            bridge.get_seq_num_for(message_types::add_tokens_on_sui()),
            false,
            vector[BTC_ID, ETH_ID, USDC_ID, USDT_ID],
            vector[
                type_name::get<BTC>().into_string(),
                type_name::get<ETH>().into_string(),
                type_name::get<USDC>().into_string(),
                type_name::get<USDT>().into_string(),
            ],
            vector[1000, 100, 1, 1],
        );
        let signatures = env.sign_message(add_token_message);
        bridge.execute_system_message(add_token_message, signatures);

        test_scenario::return_shared(bridge);
    }

    //
    // Utility functions for custom behavior
    //

    public fun token_type<T>(env: &mut BridgeEnv): u8 {
        env.scenario.next_tx(@0x0);
        let bridge = env.scenario.take_shared<Bridge>();
        let inner = bridge.test_load_inner();
        let token_id = inner.inner_treasury().token_id<T>();
        test_scenario::return_shared(bridge);
        token_id
    }

    const SUI_MESSAGE_PREFIX: vector<u8> = b"SUI_BRIDGE_MESSAGE";

    fun sign_message(
        env: &BridgeEnv,
        message: BridgeMessage,
    ): vector<vector<u8>> {
        let mut message_bytes = SUI_MESSAGE_PREFIX;
        message_bytes.append(message.serialize_message());
        let mut message_bytes = SUI_MESSAGE_PREFIX;
        message_bytes.append(message.serialize_message());
        env
            .validators
            .map_ref!(
                |validator| {
                    secp256k1_sign(
                        validator.key_pair.private_key(),
                        &message_bytes,
                        0,
                        true,
                    )
                },
            )
    }

    public fun sign_message_with(
        env: &BridgeEnv,
        message: BridgeMessage,
        validator_idxs: vector<u64>,
    ): vector<vector<u8>> {
        let mut message_bytes = SUI_MESSAGE_PREFIX;
        message_bytes.append(message.serialize_message());
        validator_idxs.map!(
            |idx| {
                secp256k1_sign(
                    env.validators[idx].key_pair.private_key(),
                    &message_bytes,
                    0,
                    true,
                )
            },
        )
    }

    public fun bridge_in_message<Token>(
        env: &mut BridgeEnv,
        source_chain: u8,
        source_address: vector<u8>,
        target_address: address,
        amount: u64,
    ): BridgeMessage {
        let token_type = env.token_type<Token>();

        let scenario = &mut env.scenario;
        scenario.next_tx(@0x0);
        let mut bridge = scenario.take_shared<Bridge>();

        let message = message::create_token_bridge_message(
            source_chain,
            bridge.get_seq_num_inc_for(message_types::token()),
            source_address,
            env.chain_id,
            address::to_bytes(target_address),
            token_type,
            amount,
        );
        test_scenario::return_shared(bridge);
        message
    }

    public fun bridge_out_message<Token>(
        env: &mut BridgeEnv,
        target_chain: u8,
        target_address: vector<u8>,
        source_address: address,
        amount: u64,
        transfer_id: u64,
    ): BridgeMessage {
        let token_type = env.token_type<Token>();

        let scenario = &mut env.scenario;
        scenario.next_tx(@0x0);
        let bridge = scenario.take_shared<Bridge>();

        let message = message::create_token_bridge_message(
            env.chain_id,
            transfer_id,
            address::to_bytes(source_address),
            target_chain,
            target_address,
            token_type,
            amount,
        );
        test_scenario::return_shared(bridge);
        message
    }

    public fun bridge_token_signed_message<Token>(
        env: &mut BridgeEnv,
        source_chain: u8,
        source_address: vector<u8>,
        target_address: address,
        amount: u64,
    ): (BridgeMessage, vector<vector<u8>>) {
        let token_type = env.token_type<Token>();
        let scenario = &mut env.scenario;
        scenario.next_tx(@0x0);
        let mut bridge = scenario.take_shared<Bridge>();
        let seq_num = bridge.get_seq_num_inc_for(message_types::token());
        test_scenario::return_shared(bridge);
        let message = message::create_token_bridge_message(
            source_chain,
            seq_num,
            source_address,
            env.chain_id,
            address::to_bytes(target_address),
            token_type,
            amount,
        );
        let signatures = env.sign_message(message);
        (message, signatures)
    }

    // Bridge the `amount` of the given `Token` from the `source_chain`.
    public fun bridge_to_sui<Token>(
        env: &mut BridgeEnv,
        source_chain: u8,
        source_address: vector<u8>,
        target_address: address,
        amount: u64,
    ): u64 {
        let token_type = env.token_type<Token>();

        // setup
        let scenario = &mut env.scenario;
        scenario.next_tx(@0x0);
        let mut bridge = scenario.take_shared<Bridge>();

        // sign message
        let seq_num = bridge.get_seq_num_inc_for(message_types::token());
        let message = message::create_token_bridge_message(
            source_chain,
            seq_num,
            source_address,
            env.chain_id,
            address::to_bytes(target_address),
            token_type,
            amount,
        );
        let signatures = env.sign_message(message);

        // run approval
        bridge.approve_token_transfer(message, signatures);

        // verify approval events
        let approved_events = event::events_by_type<TokenTransferApproved>();
        let already_approved_events = event::events_by_type<
            TokenTransferAlreadyApproved,
        >();
        assert!(
            approved_events.length() == 1 ||
            already_approved_events.length() == 1,
        );
        let key = if (approved_events.length() == 1) {
            approved_events[0].transfer_approve_key()
        } else {
            already_approved_events[0].transfer_already_approved_key()
        };
        let (sc, mt, sn) = key.unpack_message();
        assert!(source_chain == sc);
        assert!(mt == message_types::token());
        assert!(sn == seq_num);

        // tear down
        test_scenario::return_shared(bridge);
        seq_num
    }

    // Approves a token transer
    public fun approve_token_transfer(
        env: &mut BridgeEnv,
        message: BridgeMessage,
        signatures: vector<vector<u8>>,
    ): u8 {
        let msg_key = message.key();

        // set up
        let scenario = &mut env.scenario;
        scenario.next_tx(@0x0);
        let mut bridge = scenario.take_shared<Bridge>();

        // run approval
        bridge.approve_token_transfer(message, signatures);

        // verify approval events
        let approved = event::events_by_type<TokenTransferApproved>();
        let already_approved = event::events_by_type<
            TokenTransferAlreadyApproved,
        >();
        assert!(approved.length() == 1 || already_approved.length() == 1);
        let (key, approve_status) = if (approved.length() == 1) {
            (approved[0].transfer_approve_key(), APPROVED)
        } else {
            (
                already_approved[0].transfer_already_approved_key(),
                ALREADY_APPROVED,
            )
        };
        assert!(msg_key == key);

        // tear down
        test_scenario::return_shared(bridge);
        approve_status
    }

    // Clain a token transfer and returns the coin
    public fun claim_token<T>(
        env: &mut BridgeEnv,
        sender: address,
        source_chain: u8,
        bridge_seq_num: u64,
    ): Coin<T> {
        // set up
        let scenario = &mut env.scenario;
        scenario.next_tx(sender);
        let clock = &env.clock;
        let mut bridge = scenario.take_shared<Bridge>();
        let ctx = scenario.ctx();
        let total_supply_before = get_total_supply<T>(&bridge);

        // run claim
        let token = bridge.claim_token<T>(
            clock,
            source_chain,
            bridge_seq_num,
            ctx,
        );

        // verify value change and claim events
        let token_value = token.value();
        assert!(
            total_supply_before + token_value == get_total_supply<T>(&bridge),
        );
        let claimed = event::events_by_type<TokenTransferClaimed>();
        let already_claimed = event::events_by_type<
            TokenTransferAlreadyClaimed,
        >();
        let limit_exceeded = event::events_by_type<TokenTransferLimitExceed>();
        assert!(
            claimed.length() == 1 || already_claimed.length() == 1 ||
            limit_exceeded.length() == 1,
        );
        let key = if (claimed.length() == 1) {
            claimed[0].transfer_claimed_key()
        } else if (already_claimed.length() == 1) {
            already_claimed[0].transfer_already_claimed_key()
        } else {
            limit_exceeded[0].transfer_limit_exceed_key()
        };
        let (sc, mt, sn) = key.unpack_message();
        assert!(source_chain == sc);
        assert!(mt == message_types::token());
        assert!(sn == bridge_seq_num);

        // tear down
        test_scenario::return_shared(bridge);
        token
    }

    // Claim a token and transfer to the receiver in the bridge message
    public fun claim_and_transfer_token<T>(
        env: &mut BridgeEnv,
        source_chain: u8,
        bridge_seq_num: u64,
    ): u8 {
        // set up
        let sender = @0xA1B2C3; // random sender
        let scenario = &mut env.scenario;
        scenario.next_tx(sender);
        let clock = &env.clock;
        let mut bridge = scenario.take_shared<Bridge>();
        let ctx = scenario.ctx();
        let total_supply_before = get_total_supply<T>(&bridge);

        // run claim and transfer
        bridge.claim_and_transfer_token<T>(
            clock,
            source_chain,
            bridge_seq_num,
            ctx,
        );

        // verify claim events
        let claimed = event::events_by_type<TokenTransferClaimed>();
        let already_claimed = event::events_by_type<
            TokenTransferAlreadyClaimed,
        >();
        let limit_exceeded = event::events_by_type<TokenTransferLimitExceed>();
        assert!(
            claimed.length() == 1 || already_claimed.length() == 1 ||
            limit_exceeded.length() == 1,
        );
        let (key, claim_status) = if (claimed.length() == 1) {
            (claimed[0].transfer_claimed_key(), CLAIMED)
        } else if (already_claimed.length() == 1) {
            (already_claimed[0].transfer_already_claimed_key(), ALREADY_CLAIMED)
        } else {
            (limit_exceeded[0].transfer_limit_exceed_key(), LIMIT_EXCEEDED)
        };
        let (sc, mt, sn) = key.unpack_message();
        assert!(source_chain == sc);
        assert!(mt == message_types::token());
        assert!(sn == bridge_seq_num);

        // verify effects
        let effects = scenario.next_tx(@0xABCDEF);
        let created = effects.created();
        if (!created.is_empty()) {
            let token_id = effects.created()[0];
            let token = scenario.take_from_sender_by_id<Coin<T>>(token_id);
            let token_value = token.value();
            assert!(
                total_supply_before + token_value ==
                get_total_supply<T>(&bridge),
            );
            scenario.return_to_sender(token);
        };

        // tear down
        test_scenario::return_shared(bridge);
        claim_status
    }

    // Send a coin (token) to the target chain
    public fun send_token<T>(
        env: &mut BridgeEnv,
        sender: address,
        target_chain_id: u8,
        eth_address: vector<u8>,
        coin: Coin<T>,
    ): u64 {
        // set up
        let chain_id = env.chain_id;
        let scenario = env.scenario();
        scenario.next_tx(sender);
        let mut bridge = scenario.take_shared<Bridge>();
        let coin_value = coin.value();
        let total_supply_before = get_total_supply<T>(&bridge);
        let seq_num = bridge.get_seq_num_for(message_types::token());

        // run send
        bridge.send_token(target_chain_id, eth_address, coin, scenario.ctx());

        // verify send events
        assert!(
            total_supply_before - coin_value == get_total_supply<T>(&bridge),
        );
        let deposited_events = event::events_by_type<TokenDepositedEvent>();
        assert!(deposited_events.length() == 1);
        let (
            event_seq_num,
            _event_source_chain,
            _event_sender_address,
            _event_target_chain,
            _event_target_address,
            _event_token_type,
            event_amount,
        ) = deposited_events[0].unwrap_deposited_event();
        assert!(event_seq_num == seq_num);
        assert!(event_amount == coin_value);
        assert_key(chain_id, &bridge);

        // tear down
        test_scenario::return_shared(bridge);
        seq_num
    }

    // Update the limit for a given route
    public fun update_bridge_limit(
        env: &mut BridgeEnv,
        sender: address,
        receiving_chain: u8,
        sending_chain: u8,
        limit: u64,
    ): u64 {
        // set up
        let scenario = env.scenario();
        scenario.next_tx(sender);
        let mut bridge = scenario.take_shared<Bridge>();

        // message signed
        let msg = message::create_update_bridge_limit_message(
            receiving_chain,
            bridge.get_seq_num_for(message_types::update_bridge_limit()),
            sending_chain,
            limit,
        );
        let signatures = env.sign_message(msg);

        // run limit update
        bridge.execute_system_message(msg, signatures);

        // verify limit events
        let limit_events = event::events_by_type<UpdateRouteLimitEvent>();
        assert!(limit_events.length() == 1);
        let event = limit_events[0];
        let (sc, rc, new_limit) = event.unpack_route_limit_event();
        assert!(sc == sending_chain);
        assert!(rc == receiving_chain);
        assert!(new_limit == limit);

        // tear down
        test_scenario::return_shared(bridge);
        new_limit
    }

    // Update a given asset price (notional value)
    public fun update_asset_price(
        env: &mut BridgeEnv,
        sender: address,
        token_id: u8,
        value: u64,
    ) {
        // set up
        let scenario = &mut env.scenario;
        scenario.next_tx(sender);
        let mut bridge = scenario.take_shared<Bridge>();

        // message signed
        let message = message::create_update_asset_price_message(
            token_id,
            env.chain_id,
            bridge.get_seq_num_for(message_types::update_asset_price()),
            value,
        );
        let signatures = env.sign_message(message);

        // run price update
        bridge.execute_system_message(message, signatures);

        // verify price events
        let update_events = event::events_by_type<UpdateTokenPriceEvent>();
        assert!(update_events.length() == 1);
        let (event_token_id, event_new_price) = update_events[
            0
        ].unwrap_update_event();
        assert!(event_token_id == token_id);
        assert!(event_new_price == value);

        // tear down
        test_scenario::return_shared(bridge);
    }

    // Register the `TEST_TOKEN` token
    public fun register_test_token(env: &mut BridgeEnv) {
        // set up
        let scenario = &mut env.scenario;
        scenario.next_tx(@0x0);
        let mut bridge = scenario.take_shared<Bridge>();

        // "create" the `Coin`
        let (
            upgrade_cap,
            treasury_cap,
            metadata,
        ) = test_token::create_bridge_token(scenario.ctx());
        // register the coin/token with the bridge
        bridge.register_foreign_token<TEST_TOKEN>(
            treasury_cap,
            upgrade_cap,
            &metadata,
        );

        // verify registration events
        let register_events = event::events_by_type<TokenRegistrationEvent>();
        assert!(register_events.length() == 1);
        let (type_name, decimal, nat) = register_events[
            0
        ].unwrap_registration_event();
        assert!(type_name == type_name::get<TEST_TOKEN>());
        assert!(decimal == 8);
        assert!(nat == false);

        // tear down
        destroy(metadata);
        test_scenario::return_shared(bridge);
    }

    // Add a list of tokens to the bridge.
    public fun add_tokens(
        env: &mut BridgeEnv,
        sender: address,
        native_token: bool,
        token_ids: vector<u8>,
        type_names: vector<String>,
        token_prices: vector<u64>,
    ) {
        // set up
        let scenario = &mut env.scenario;
        scenario.next_tx(sender);
        let mut bridge = scenario.take_shared<Bridge>();

        // message signed
        let message = create_add_tokens_on_sui_message(
            env.chain_id,
            bridge.get_seq_num_for(message_types::add_tokens_on_sui()),
            native_token,
            token_ids,
            type_names,
            token_prices,
        );
        let signatures = env.sign_message(message);

        // run token addition
        bridge.execute_system_message(message, signatures);

        // verify token addition events
        let new_tokens_events = event::events_by_type<NewTokenEvent>();
        assert!(new_tokens_events.length() <= token_ids.length());

        // tear down
        test_scenario::return_shared(bridge);
    }

    // Blocklist a list of bridge nodes
    public fun execute_blocklist(
        env: &mut BridgeEnv,
        sender: address,
        chain_id: u8,
        blocklist_type: u8,
        validator_ecdsa_addresses: vector<vector<u8>>,
    ) {
        // set up
        let scenario = env.scenario();
        scenario.next_tx(sender);
        let mut bridge = scenario.take_shared<Bridge>();

        // message signed
        let blocklist = create_blocklist_message(
            chain_id,
            bridge.get_seq_num_for(message_types::committee_blocklist()),
            blocklist_type,
            validator_ecdsa_addresses,
        );
        let signatures = env.sign_message(blocklist);

        // run blocklist
        bridge.execute_system_message(blocklist, signatures);

        // verify blocklist events
        let block_list_events = event::events_by_type<
            BlocklistValidatorEvent,
        >();
        assert!(
            block_list_events.length() == validator_ecdsa_addresses.length(),
        );

        // tear down
        test_scenario::return_shared(bridge);
    }

    // Register new token
    public fun register_foreign_token<T>(
        env: &mut BridgeEnv,
        treasury_cap: TreasuryCap<T>,
        upgrade_cap: UpgradeCap,
        metadata: CoinMetadata<T>,
        sender: address,
    ) {
        // set up
        let scenario = env.scenario();
        scenario.next_tx(sender);
        let mut bridge = scenario.take_shared<Bridge>();

        // run registration
        bridge.register_foreign_token<T>(treasury_cap, upgrade_cap, &metadata);

        // verify registration events
        let register_events = event::events_by_type<TokenRegistrationEvent>();
        assert!(register_events.length() == 1);

        // verify changes in bridge
        let type_name = type_name::get<T>();
        let inner = bridge.test_load_inner();
        let treasury = inner.inner_treasury();
        let waiting_room = treasury.waiting_room();
        assert!(waiting_room.contains(type_name::into_string(type_name)));
        let treasuries = treasury.treasuries();
        assert!(treasuries.contains(type_name));

        // tear down
        test_scenario::return_shared(bridge);
        destroy(metadata);
    }

    // Freeze the bridge
    public fun freeze_bridge(env: &mut BridgeEnv, sender: address, error: u64) {
        // set up
        let scenario = env.scenario();
        scenario.next_tx(sender);
        let mut bridge = scenario.take_shared<Bridge>();
        let seq_num = bridge.get_seq_num_for(message_types::emergency_op());

        // message signed
        let msg = message::create_emergency_op_message(
            env.chain_id,
            seq_num,
            emergency_op_pause(),
        );
        let signatures = env.sign_message(msg);

        // run freeze
        bridge.execute_system_message(msg, signatures);

        // verify freeze events
        let register_events = event::events_by_type<EmergencyOpEvent>();
        assert!(register_events.length() == 1);
        assert!(register_events[0].unwrap_emergency_op_event() == true);

        // verify freeze
        let inner = bridge.test_load_inner_mut();
        inner.assert_paused(error);

        // tear down
        test_scenario::return_shared(bridge);
    }

    // Unfreeze the bridge
    public fun unfreeze_bridge(
        env: &mut BridgeEnv,
        sender: address,
        error: u64,
    ) {
        // set up
        let scenario = env.scenario();
        scenario.next_tx(sender);
        let mut bridge = scenario.take_shared<Bridge>();
        let seq_num = bridge.get_seq_num_for(message_types::emergency_op());

        // message signed
        let msg = message::create_emergency_op_message(
            env.chain_id,
            seq_num,
            emergency_op_unpause(),
        );
        let signatures = env.sign_message(msg);

        // run unfreeze
        bridge.execute_system_message(msg, signatures);
        let register_events = event::events_by_type<EmergencyOpEvent>();
        assert!(register_events.length() == 1);
        assert!(register_events[0].unwrap_emergency_op_event() == false);

        // verify unfreeze events

        // verify unfreeze
        let inner = bridge.test_load_inner_mut();
        inner.assert_not_paused(error);

        // tear down
        test_scenario::return_shared(bridge);
    }

    //
    // Getters
    //

    public fun ctx(env: &mut BridgeEnv): &mut TxContext {
        env.scenario.ctx()
    }

    public fun scenario(env: &mut BridgeEnv): &mut Scenario {
        &mut env.scenario
    }

    public fun chain_id(env: &mut BridgeEnv): u8 {
        env.chain_id
    }

    public fun validators(env: &BridgeEnv): &vector<ValidatorInfo> {
        &env.validators
    }

    public fun get_btc(env: &mut BridgeEnv, amount: u64): Coin<BTC> {
        let scenario = &mut env.scenario;
        let ctx = scenario.ctx();
        env.vault.btc_coins.split(amount, ctx)
    }

    public fun get_eth(env: &mut BridgeEnv, amount: u64): Coin<ETH> {
        let scenario = &mut env.scenario;
        let ctx = scenario.ctx();
        env.vault.eth_coins.split(amount, ctx)
    }

    public fun get_usdc(env: &mut BridgeEnv, amount: u64): Coin<USDC> {
        let scenario = &mut env.scenario;
        let ctx = scenario.ctx();
        env.vault.usdc_coins.split(amount, ctx)
    }

    public fun get_usdt(env: &mut BridgeEnv, amount: u64): Coin<USDT> {
        let scenario = &mut env.scenario;
        let ctx = scenario.ctx();
        env.vault.usdt_coins.split(amount, ctx)
    }

    public fun limits(env: &mut BridgeEnv, dest: u8): u64 {
        let scenario = env.scenario();
        scenario.next_tx(@0x0);
        let bridge = scenario.take_shared<Bridge>();
        let route = chain_ids::get_route(dest, env.chain_id);
        let limits = bridge
            .test_load_inner()
            .inner_limiter()
            .get_route_limit(&route);
        test_scenario::return_shared(bridge);
        limits
    }

    fun assert_key(chain_id: u8, bridge: &Bridge) {
        let inner = bridge.test_load_inner();
        let transfer_record = inner.inner_token_transfer_records();
        let seq_num = inner.sequence_nums()[&message_types::token()] - 1;
        let key = message::create_key(
            chain_id,
            message_types::token(),
            seq_num,
        );
        assert!(transfer_record.contains(key));
    }

    //
    // Internal functions
    //

    // Destroy the vault
    fun destroy_valut(vault: Vault) {
        let Vault {
            btc_coins,
            eth_coins,
            usdc_coins,
            usdt_coins,
            test_coins,
        } = vault;
        btc_coins.burn_for_testing();
        eth_coins.burn_for_testing();
        usdc_coins.burn_for_testing();
        usdt_coins.burn_for_testing();
        test_coins.burn_for_testing();
    }

    // Load the vault with some coins
    fun load_vault(env: &mut BridgeEnv, sender: address) {
        let scenario = &mut env.scenario;
        scenario.next_tx(sender);
        let mut bridge = scenario.take_shared<Bridge>();
        let vault = &mut env.vault;
        vault.btc_coins.join(mint_some(&mut bridge, scenario.ctx()));
        vault.eth_coins.join(mint_some(&mut bridge, scenario.ctx()));
        vault.usdc_coins.join(mint_some(&mut bridge, scenario.ctx()));
        vault.usdt_coins.join(mint_some(&mut bridge, scenario.ctx()));
        test_scenario::return_shared(bridge);
    }

    // Mint some coins
    fun mint_some<T>(bridge: &mut Bridge, ctx: &mut TxContext): Coin<T> {
        let treasury = bridge.test_load_inner_mut().inner_treasury_mut();
        let coin = treasury.mint<T>(1_000_000, ctx);
        coin
    }

    fun get_total_supply<T>(bridge: &Bridge): u64 {
        let inner = bridge.test_load_inner();
        let treasury = inner.inner_treasury();
        let treasuries = treasury.treasuries();
        let tc: &TreasuryCap<T> = &treasuries[type_name::get<T>()];
        tc.total_supply()
    }
}

//
// Test Coins
//

#[test_only]
module bridge::test_token {
    use std::ascii;
    use std::type_name;
    use sui::address;
    use sui::coin::{CoinMetadata, TreasuryCap, create_currency};
    use sui::hex;
    use sui::package::{UpgradeCap, test_publish};
    use sui::test_utils::create_one_time_witness;

    public struct TEST_TOKEN has drop {}

    public fun create_bridge_token(
        ctx: &mut TxContext,
    ): (UpgradeCap, TreasuryCap<TEST_TOKEN>, CoinMetadata<TEST_TOKEN>) {
        let otw = create_one_time_witness<TEST_TOKEN>();
        let (treasury_cap, metadata) = create_currency(
            otw,
            8,
            b"tst",
            b"test",
            b"bridge test token",
            option::none(),
            ctx,
        );

        let type_name = type_name::get<TEST_TOKEN>();
        let address_bytes = hex::decode(
            ascii::into_bytes(type_name::get_address(&type_name)),
        );
        let coin_id = address::from_bytes(address_bytes).to_id();
        let upgrade_cap = test_publish(coin_id, ctx);

        (upgrade_cap, treasury_cap, metadata)
    }
}

#[test_only]
module bridge::btc {
    use std::ascii;
    use std::type_name;
    use sui::address;
    use sui::coin::{CoinMetadata, TreasuryCap, create_currency};
    use sui::hex;
    use sui::package::{UpgradeCap, test_publish};
    use sui::test_utils::create_one_time_witness;

    public struct BTC has drop {}

    public fun create_bridge_token(
        ctx: &mut TxContext,
    ): (UpgradeCap, TreasuryCap<BTC>, CoinMetadata<BTC>) {
        let otw = create_one_time_witness<BTC>();
        let (treasury_cap, metadata) = create_currency(
            otw,
            8,
            b"btc",
            b"bitcoin",
            b"bridge bitcoin token",
            option::none(),
            ctx,
        );

        let type_name = type_name::get<BTC>();
        let address_bytes = hex::decode(
            ascii::into_bytes(type_name::get_address(&type_name)),
        );
        let coin_id = address::from_bytes(address_bytes).to_id();
        let upgrade_cap = test_publish(coin_id, ctx);

        (upgrade_cap, treasury_cap, metadata)
    }
}

#[test_only]
module bridge::eth {
    use std::ascii;
    use std::type_name;
    use sui::address;
    use sui::coin::{CoinMetadata, TreasuryCap, create_currency};
    use sui::hex;
    use sui::package::{UpgradeCap, test_publish};
    use sui::test_utils::create_one_time_witness;

    public struct ETH has drop {}

    public fun create_bridge_token(
        ctx: &mut TxContext,
    ): (UpgradeCap, TreasuryCap<ETH>, CoinMetadata<ETH>) {
        let otw = create_one_time_witness<ETH>();
        let (treasury_cap, metadata) = create_currency(
            otw,
            8,
            b"eth",
            b"eth",
            b"bridge ethereum token",
            option::none(),
            ctx,
        );

        let type_name = type_name::get<ETH>();
        let address_bytes = hex::decode(
            ascii::into_bytes(type_name::get_address(&type_name)),
        );
        let coin_id = address::from_bytes(address_bytes).to_id();
        let upgrade_cap = test_publish(coin_id, ctx);

        (upgrade_cap, treasury_cap, metadata)
    }
}

#[test_only]
module bridge::usdc {
    use std::ascii;
    use std::type_name;
    use sui::address;
    use sui::coin::{CoinMetadata, TreasuryCap, create_currency};
    use sui::hex;
    use sui::package::{UpgradeCap, test_publish};
    use sui::test_utils::create_one_time_witness;

    public struct USDC has drop {}

    public fun create_bridge_token(
        ctx: &mut TxContext,
    ): (UpgradeCap, TreasuryCap<USDC>, CoinMetadata<USDC>) {
        let otw = create_one_time_witness<USDC>();
        let (treasury_cap, metadata) = create_currency(
            otw,
            6,
            b"usdc",
            b"usdc",
            b"bridge usdc token",
            option::none(),
            ctx,
        );

        let type_name = type_name::get<USDC>();
        let address_bytes = hex::decode(
            ascii::into_bytes(type_name::get_address(&type_name)),
        );
        let coin_id = address::from_bytes(address_bytes).to_id();
        let upgrade_cap = test_publish(coin_id, ctx);

        (upgrade_cap, treasury_cap, metadata)
    }
}

#[test_only]
module bridge::usdt {
    use std::ascii;
    use std::type_name;
    use sui::address;
    use sui::coin::{CoinMetadata, TreasuryCap, create_currency};
    use sui::hex;
    use sui::package::{UpgradeCap, test_publish};
    use sui::test_utils::create_one_time_witness;

    public struct USDT has drop {}

    public fun create_bridge_token(
        ctx: &mut TxContext,
    ): (UpgradeCap, TreasuryCap<USDT>, CoinMetadata<USDT>) {
        let otw = create_one_time_witness<USDT>();
        let (treasury_cap, metadata) = create_currency(
            otw,
            6,
            b"usdt",
            b"usdt",
            b"bridge usdt token",
            option::none(),
            ctx,
        );

        let type_name = type_name::get<USDT>();
        let address_bytes = hex::decode(
            ascii::into_bytes(type_name::get_address(&type_name)),
        );
        let coin_id = address::from_bytes(address_bytes).to_id();
        let upgrade_cap = test_publish(coin_id, ctx);

        (upgrade_cap, treasury_cap, metadata)
    }
}
