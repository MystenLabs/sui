// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module fork_demo::fork_tests {
    use sui::coin::{Self, Coin};
    use sui::test_scenario as ts;
    use fork_demo::demo_coin::{Self, DEMO_COIN};

    const ADMIN: address = @0xAD;
    const USER1: address = @0x1;
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
}
