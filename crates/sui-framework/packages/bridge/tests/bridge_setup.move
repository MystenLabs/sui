// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module bridge::bridge_setup {
    use bridge::bridge::{
        assert_not_paused, assert_paused, create_bridge_for_testing, inner_token_transfer_records,
        test_execute_add_tokens_on_sui, test_execute_emergency_op, test_init_bridge_committee,
        test_load_inner_mut, Bridge,
    };
    use bridge::btc::{Self, BTC};
    use bridge::eth::{Self, ETH};
    use bridge::message::{
        Self, create_add_tokens_on_sui_message, create_blocklist_message, emergency_op_pause,
        emergency_op_unpause,
    };
    use bridge::message_types;
    use bridge::test_token::TEST_TOKEN;
    use bridge::usdc::{Self, USDC};
    use bridge::usdt::{Self, USDT};
    use std::{ascii::String, type_name};
    use sui::coin::{Self, Coin, CoinMetadata, TreasuryCap};
    use sui::hex;
    use sui::package::UpgradeCap;
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
    // Validators setup and info
    //

    const VALIDATOR1_PUBKEY: vector<u8> = b"029bef8d556d80e43ae7e0becb3a7e6838b95defe45896ed6075bb9035d06c9964";
    const VALIDATOR2_PUBKEY: vector<u8> = b"033e99a541db69bd32040dfe5037fbf5210dafa8151a71e21c5204b05d95ce0a62";
    const VALIDATOR3_PUBKEY: vector<u8> = b"033e99a541db69bd32040dfe5037fbf5210dafa8151a71e21c5204b05d95ce0a63";

    // Bridge environemnt
    public struct BridgeEnv {
        scenario: Scenario,
        chain_id: u8,
        seq_num: u64,
        vault: Vault,
    }

    public struct Vault {
        btc_coins: Coin<BTC>,
        eth_coins: Coin<ETH>,
        usdc_coins: Coin<USDC>,
        usdt_coins: Coin<USDT>,
        test_coins: Coin<TEST_TOKEN>,
    }

    // Info to set up a validator
    public struct ValidatorInfo has copy, drop {
        validator: address,
        stake_amount: u64,
    }

    // HotPotato to access the Bridge
    public struct BridgeWrapper {
        bridge: Bridge,
    }

    //
    // Public functions
    //

    //
    // Environment creation and destruction
    //

    public fun create_env(chain_id: u8, start_addr: address): BridgeEnv {
        let mut scenario = test_scenario::begin(start_addr);
        let ctx = scenario.ctx();
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
            seq_num: 0,
            vault,
        }
    }

    public fun destroy_env(env: BridgeEnv) {
        let BridgeEnv {scenario, chain_id: _, seq_num: _, vault} = env;
        destroy_valut(vault);
        scenario.end();
    }

    public fun create_validator_info(validator: address, stake_amount: u64): ValidatorInfo {
        ValidatorInfo {
            validator,
            stake_amount,
        }
    }

    //
    // Add a set of validators to the chain.
    // Call only once in a test scenario.
    public fun setup_validators(
        env: &mut BridgeEnv,
        validators_info: vector<ValidatorInfo>,
        sender: address,
    ) {
        let scenario = &mut env.scenario;
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

    //
    // Bridge creation and setup
    //

    // Set up an environment with 3 validators, a bridge with
    // a treasury and a committee with all 3 validators.
    // The treasury will contain 4 tokens: ETH, BTC, USDT, USDC.
    // Save the Bridge as a shared object.
    public fun create_bridge_default(env: &mut BridgeEnv) {
        let validators = vector[
            ValidatorInfo { validator: @0xA, stake_amount: 100 },
            ValidatorInfo { validator: @0xB, stake_amount: 100 },
            ValidatorInfo { validator: @0xC, stake_amount: 100 },
        ];
        let sender = @0x0;
        env.setup_validators(validators, sender);
        env.create_bridge(sender);
        env.register_committee();
        env.init_committee(sender);
    }

    // Create a bridge and set up a treasury.
    // The treasury will contain 4 tokens: ETH, BTC, USDT, USDC.
    // Save the Bridge as a shared object.
    // No operation on the validators.
    public fun create_bridge(env: &mut BridgeEnv, sender: address) {
        env.scenario.next_tx(sender);
        let ctx = env.scenario.ctx();
        create_bridge_for_testing(object::new(ctx), env.chain_id, ctx);
        env.setup_treasury(sender);
    }

    // Register 3 committee members (validators `@0xA`, `@0xB`, `@0xC`)
    public fun register_committee(env: &mut BridgeEnv) {
        let scenario = &mut env.scenario;
        scenario.next_tx(@0x0);
        let mut bridge = scenario.take_shared<Bridge>();
        let mut system_state = test_scenario::take_shared<SuiSystemState>(scenario);

        // register committee member `@0xA`
        scenario.next_tx(@0xA);
        bridge.committee_registration(
            &mut system_state,
            hex::decode(VALIDATOR1_PUBKEY),
            b"",
            scenario.ctx(),
        );

        // register committee member `@0xB`
        scenario.next_tx(@0xB);
        bridge.committee_registration(
            &mut system_state,
            hex::decode(VALIDATOR2_PUBKEY),
            b"",
            scenario.ctx(),
        );

        // register committee member `@0xC`
        scenario.next_tx(@0xC);
        bridge.committee_registration(
            &mut system_state,
            hex::decode(VALIDATOR3_PUBKEY),
            b"",
            scenario.ctx(),
        );

        test_scenario::return_shared(bridge);
        test_scenario::return_shared(system_state);
    }

    // Init the bridge committee
    public fun init_committee(env: &mut BridgeEnv, sender: address) {
        let scenario = &mut env.scenario;
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

    // Set up a treasury with 4 tokens: ETH, BTC, USDT, USDC.
    public fun setup_treasury(env: &mut BridgeEnv, sender: address) {
        env.register_default_tokens(sender);
        env.add_default_tokens(sender);
        env.load_vault(sender);
    }

    // Register 4 tokens with the Bridge: ETH, BTC, USDT, USDC.
    public fun register_default_tokens(env: &mut BridgeEnv, sender: address) {
        env.scenario.next_tx(sender);
        let mut bridge = env.scenario.take_shared<Bridge>();

        // BTC
        let (upgrade_cap, treasury_cap, metadata) =
            btc::create_bridge_token(env.scenario.ctx());
        bridge.register_foreign_token<BTC>(
            treasury_cap,
            upgrade_cap,
            &metadata,
        );
        destroy(metadata);
        // ETH
        let (upgrade_cap, treasury_cap, metadata) =
            eth::create_bridge_token(env.scenario.ctx());
        bridge.register_foreign_token<ETH>(
            treasury_cap,
            upgrade_cap,
            &metadata,
        );
        destroy(metadata);
        // USDC
        let (upgrade_cap, treasury_cap, metadata) =
            usdc::create_bridge_token(env.scenario.ctx());
        bridge.register_foreign_token<USDC>(
            treasury_cap,
            upgrade_cap,
            &metadata,
        );
        destroy(metadata);
        // USDT
        let (upgrade_cap, treasury_cap, metadata) =
            usdt::create_bridge_token(env.scenario.ctx());
        bridge.register_foreign_token<USDT>(
            treasury_cap,
            upgrade_cap,
            &metadata,
        );
        destroy(metadata);

        test_scenario::return_shared(bridge);
    }

    // Add the 4 tokens previously registered: ETH, BTC, USDT, USDC.
    public fun add_default_tokens(env: &mut BridgeEnv, sender: address) {
        let scenario = &mut env.scenario;
        scenario.next_tx(sender);
        let mut bridge = scenario.take_shared<Bridge>();

        let add_token_message = create_add_tokens_on_sui_message(
            env.chain_id,
            env.seq_num(),
            false, // native_token
            vector[BTC_ID, ETH_ID, USDC_ID, USDT_ID],
            vector[
                type_name::get<BTC>().into_string(),
                type_name::get<ETH>().into_string(),
                type_name::get<USDC>().into_string(),
                type_name::get<USDT>().into_string(),
            ],
            vector[1000, 100, 1, 1],
        );
        let payload = add_token_message.extract_add_tokens_on_sui();
        bridge.test_execute_add_tokens_on_sui(payload);

        test_scenario::return_shared(bridge);
    }

    // Add the 4 tokens previously registered: ETH, BTC, USDT, USDC.
    public fun add_tokens(
        env: &mut BridgeEnv, 
        sender: address,
        native_token: bool,
        token_ids: vector<u8>,
        type_names: vector<String>,
        token_prices: vector<u64>,
    ) {
        let scenario = &mut env.scenario;
        scenario.next_tx(sender);
        let mut bridge = scenario.take_shared<Bridge>();

        let add_token_message = create_add_tokens_on_sui_message(
            env.chain_id,
            env.seq_num(),
            native_token,
            token_ids,
            type_names,
            token_prices,
        );
        let payload = add_token_message.extract_add_tokens_on_sui();
        bridge.test_execute_add_tokens_on_sui(payload);

        test_scenario::return_shared(bridge);
    }

    public fun update_asset_price(
        env: &mut BridgeEnv, 
        sender: address,
        token_id: u8,
        value:u64,
    ) {
        let scenario = &mut env.scenario;
        scenario.next_tx(sender);
        let mut bridge = scenario.take_shared<Bridge>();
        let inner = bridge.test_load_inner_mut();

        let msg = message::create_update_asset_price_message(
            token_id,
            env.chain_id,
            env.seq_num(),
            value,
        );
        let payload = msg.extract_update_asset_price();
        inner.test_execute_update_asset_price(payload);

        test_scenario::return_shared(bridge);
    }

    //
    // Getters
    //

    public fun validator_pubkeys(): vector<vector<u8>> {
        vector[
            VALIDATOR1_PUBKEY,
            VALIDATOR2_PUBKEY,
            VALIDATOR3_PUBKEY,
        ]
    }

    public fun ctx(env: &mut BridgeEnv): &mut TxContext {
        env.scenario.ctx()
    }

    public fun scenario(env: &mut BridgeEnv): &mut Scenario {
        &mut env.scenario
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

    public fun return_bridge(bridge: BridgeWrapper) {
        let BridgeWrapper { bridge } = bridge;
        test_scenario::return_shared(bridge);
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

    //
    // Bridge commands
    //

    // Register new tokens
    public fun register_foreign_token<T>(
        env: &mut BridgeEnv,
        treasury_cap: TreasuryCap<T>,
        upgrade_cap: UpgradeCap,
        metadata: CoinMetadata<T>,
        sender: address,
    ) {
        let scenario = env.scenario();
        scenario.next_tx(sender);
        let mut bridge = scenario.take_shared<Bridge>();
        bridge.register_foreign_token<T>(treasury_cap, upgrade_cap, &metadata);

        // assert changes in bridge
        let type_name = type_name::get<T>();
        let inner = bridge.test_load_inner();
        let treasury = inner.inner_treasury();
        let waiting_room = treasury.waiting_room();
        assert!(waiting_room.contains(type_name::into_string(type_name)));
        let treasuries = treasury.treasuries();
        assert!(treasuries.contains(type_name));

        test_scenario::return_shared(bridge);
        destroy(metadata);
    }

    // Freeze the bridge
    public fun freeze_bridge(env: &mut BridgeEnv, sender: address, error: u64) {
        let scenario = env.scenario();
        scenario.next_tx(sender);
        let mut bridge = scenario.take_shared<Bridge>();
        let inner = bridge.test_load_inner_mut();
        let msg = message::create_emergency_op_message(env.chain_id, 0, emergency_op_pause());
        let payload = msg.extract_emergency_op_payload();
        inner.test_execute_emergency_op(payload);
        inner.assert_paused(error);
        test_scenario::return_shared(bridge);
    }

    // Unfreeze the bridge
    public fun unfreeze_bridge(env: &mut BridgeEnv, sender: address, error: u64) {
        let scenario = env.scenario();
        scenario.next_tx(sender);
        let mut bridge = scenario.take_shared<Bridge>();
        let inner = bridge.test_load_inner_mut();
        let msg = message::create_emergency_op_message(env.chain_id, 1, emergency_op_unpause());
        let payload = msg.extract_emergency_op_payload();
        inner.test_execute_emergency_op(payload);
        inner.assert_not_paused(error);
        test_scenario::return_shared(bridge);
    }

    public fun send_token<T>(
        env: &mut BridgeEnv,
        target_chain_id: u8,
        eth_address: vector<u8>,
        coin: Coin<T>,
    ) {
        let scenario = env.scenario();
        scenario.next_tx(@0xAAAA);
        let mut bridge = scenario.take_shared<Bridge>();
        let coin_value = coin.value();
        let total_supply_before = get_total_supply<T>(&bridge);

        bridge.send_token(target_chain_id, eth_address, coin, scenario.ctx());

        assert!(total_supply_before - coin_value == get_total_supply<T>(&bridge));

        let inner = bridge.test_load_inner();
        let transfer_record = inner.inner_token_transfer_records();
        let seq_num = inner.sequence_nums()[&message_types::token()] - 1; 
        let key = message::create_key(env.chain_id, message_types::token(), seq_num);
        assert!(transfer_record.contains(key));

        test_scenario::return_shared(bridge);
    }

    public fun execute_blocklist(
        env: &mut BridgeEnv,
        sender: address,
        chain_id: u8,
        blocklist_type: u8,
        validator_ecdsa_addresses: vector<vector<u8>>,
        signatures: vector<vector<u8>>,
    ) {
        let scenario = env.scenario();
        scenario.next_tx(sender);
        let mut bridge = scenario.take_shared<Bridge>();
        let blocklist = create_blocklist_message(
            chain_id,
            env.seq_num(),
            blocklist_type,
            validator_ecdsa_addresses,
        );
        bridge.execute_system_message(blocklist, signatures);
        test_scenario::return_shared(bridge);
    }

    public fun update_bridge_limit(
        env: &mut BridgeEnv,
        sender: address,
        receiving_chain: u8,
        sending_chain: u8,
        limit: u64,
    ) {
        let scenario = env.scenario();
        scenario.next_tx(sender);
        let mut bridge = scenario.take_shared<Bridge>();
        let msg = message::create_update_bridge_limit_message(
            receiving_chain,
            env.seq_num(),
            sending_chain,
            limit,
        );
        let payload = msg.extract_update_bridge_limit();
        bridge.test_load_inner_mut().test_execute_update_bridge_limit(payload);
        test_scenario::return_shared(bridge);
    }

    //
    // Internal functions
    //

    fun seq_num(env: &mut BridgeEnv): u64 {
        let seq_num = env.seq_num;
        env.seq_num = seq_num + 1;
        seq_num
    }

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
    use std::{ascii, type_name};
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
        let address_bytes = hex::decode(ascii::into_bytes(type_name::get_address(&type_name)));
        let coin_id = address::from_bytes(address_bytes).to_id();
        let upgrade_cap = test_publish(coin_id, ctx);

        (upgrade_cap, treasury_cap, metadata)
    }
}

#[test_only]
module bridge::btc {
    use std::{ascii, type_name};
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
        let address_bytes = hex::decode(ascii::into_bytes(type_name::get_address(&type_name)));
        let coin_id = address::from_bytes(address_bytes).to_id();
        let upgrade_cap = test_publish(coin_id, ctx);

        (upgrade_cap, treasury_cap, metadata)
    }
}

#[test_only]
module bridge::eth {
    use std::{ascii, type_name};
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
        let address_bytes = hex::decode(ascii::into_bytes(type_name::get_address(&type_name)));
        let coin_id = address::from_bytes(address_bytes).to_id();
        let upgrade_cap = test_publish(coin_id, ctx);

        (upgrade_cap, treasury_cap, metadata)
    }
}

#[test_only]
module bridge::usdc {
    use std::{ascii, type_name};
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
        let address_bytes = hex::decode(ascii::into_bytes(type_name::get_address(&type_name)));
        let coin_id = address::from_bytes(address_bytes).to_id();
        let upgrade_cap = test_publish(coin_id, ctx);

        (upgrade_cap, treasury_cap, metadata)
    }
}

#[test_only]
module bridge::usdt {
    use std::{ascii, type_name};
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
        let address_bytes = hex::decode(ascii::into_bytes(type_name::get_address(&type_name)));
        let coin_id = address::from_bytes(address_bytes).to_id();
        let upgrade_cap = test_publish(coin_id, ctx);

        (upgrade_cap, treasury_cap, metadata)
    }
}
