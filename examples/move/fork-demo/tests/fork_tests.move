// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module fork_demo::fork_tests {
    use sui::coin::{Self, Coin};
    use sui::test_scenario as ts;
    use sui::clock::{Self, Clock};
    use sui::dynamic_field::{Self as df};
    use sui::dynamic_object_field::{Self as dof};
    use fork_demo::demo_coin::{Self, DEMO_COIN, DEMO_STATE, DEMO_DYNAMIC};
    use std::debug;

    const ADMIN: address = @0xAD;
    const USER1: address = @0x1111111111111111111111111111111111111111111111111111111111111111;
    const MINT_AMOUNT: u64 = 1_000_000;

    #[test]
    fun test_normal_mint_without_fork() {
        let mut scenario = ts::begin(ADMIN);

        // Initialize the coin
        demo_coin::init_for_testing(ts::ctx(&mut scenario));
        ts::next_tx(&mut scenario, ADMIN);

        // Mint coins
        {
            let mut treasury = ts::take_from_sender<coin::TreasuryCap<DEMO_COIN>>(&scenario);
            demo_coin::mint(&mut treasury, MINT_AMOUNT, USER1, ts::ctx(&mut scenario));
            ts::return_to_sender(&scenario, treasury);
        };

        ts::next_tx(&mut scenario, USER1);

        // Verify USER1 received the coins
        {
            let coin = ts::take_from_sender<Coin<DEMO_COIN>>(&scenario);
            assert!(coin::value(&coin) == MINT_AMOUNT, 0);
            ts::return_to_sender(&scenario, coin);
        };

        ts::end(scenario);
    }

    #[test]
    fun test_verify_balance_from_fork() {
        let scenario = ts::begin(USER1);

        // When testing with --fork-checkpoint, this test expects
        // USER1 to already have DEMO_COIN from the checkpoint state
        let coin = ts::take_from_sender<Coin<DEMO_COIN>>(&scenario);
        let balance = coin::value(&coin);

        // Verify the balance matches what was minted
        assert!(balance == MINT_AMOUNT, 1);

        ts::return_to_sender(&scenario, coin);

        ts::end(scenario);
    }

    #[test]
    fun test_conditional_on_fork_state() {
        let scenario = ts::begin(USER1);

        // This test demonstrates how to write tests that work both
        // with and without checkpoint forking
        // Fork mode: verify existing balance
        let coin = ts::take_from_sender<Coin<DEMO_COIN>>(&scenario);
        let balance = coin::value(&coin);
        assert!(balance > 0, 2);
        ts::return_to_sender(&scenario, coin);

        ts::end(scenario);
    }

    #[test]
    fun test_check_demo_state_on_fork_state() {
        let scenario = ts::begin(USER1);
        let s = &scenario;
        let demo_state = s.take_shared<DEMO_STATE>();
        assert!(demo_coin::get_demo_counter(&demo_state) == 1, 3);
        ts::return_shared(demo_state);
        ts::end(scenario);
    }

    #[test]
    fun test_load_sui_system_shared_object_from_fork() {
        let scenario = ts::begin(USER1);
        let s = &scenario;
        let clock = s.take_shared<Clock>();
        assert!(clock.timestamp_ms() > 0, 4);
        ts::return_shared(clock);
        ts::end(scenario);
    }

    #[test]
    fun test_borrow_dynamic_field_from_fork() {
        let scenario = ts::begin(USER1);
        let s = &scenario;

        let demo_state = s.take_shared<DEMO_STATE>();
        
        // Verify the parent object state was correctly loaded from testnet
        let current_counter = demo_coin::get_demo_counter(&demo_state);
        assert!(current_counter == 1, 6); // Counter should be 1 after add_demo_dynamic was called
        
        // The dynamic field should be automatically loaded and accessible
        let has_field = demo_coin::has_demo_dynamic(&demo_state, 0);
        assert!(has_field, 7); // Dynamic field should be accessible from fork
        
        // Access the dynamic field that was created on testnet
        let demo_dynamic = demo_coin::borrow_demo_dynamic(&demo_state, 0);
        assert!(demo_coin::get_demo_dynamic_counter(demo_dynamic) == 0, 5);

        ts::return_shared(demo_state);
        ts::end(scenario);
    }
}
