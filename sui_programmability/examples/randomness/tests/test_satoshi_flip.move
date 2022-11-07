// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module randomness::test_satoshi_flip {
    // imports
    use std::hash::sha3_256;

    use sui::coin::{Self, Coin};
    use sui::sui::SUI;
    use sui::transfer;
    use sui::test_scenario;
    use sui::tx_context::TxContext;

    use randomness::satoshi_flip::{Self, Game};


    const EWrongPlayerA: u64 = 0;
    const EWrongMinBet: u64 = 1;
    const EWrongMaxBet: u64 = 2;
    const EWrongPlayerATotal: u64 = 3;
    const EWrongOutcome: u64 = 4;


    fun init(ctx: &mut TxContext, playerA: address, playerB: address) {
        // send coins to players
        let coinA = coin::mint_for_testing<SUI>(1000, ctx);
        let coinB = coin::mint_for_testing<SUI>(300, ctx);
        transfer::transfer(coinA, playerA);
        transfer::transfer(coinB, playerB);
    }

    #[test]
    fun playerA_wins_test() {
        let world = @0x1EE7; // needed only for beginning the test_scenario
        let playerA = @0xBAE;
        let playerB = @0xFAB;
        let secret = vector[1,0,0,1,1,1,0,0,1,0,0,1,0,1];
        let secret_hash = sha3_256(secret);
        let min_bet = 100;

        let scenario_val = test_scenario::begin(world);
        let scenario = &mut scenario_val;
        {
            init(test_scenario::ctx(scenario), playerA, playerB);
        };

        // player A creates the game
        test_scenario::next_tx(scenario, playerA);
        {
            let coinA = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            let ctx = test_scenario::ctx(scenario);
            satoshi_flip::start_game(secret_hash, coinA, min_bet, ctx);
        };

        // player B checks the game details and places a bet
        test_scenario::next_tx(scenario, playerB);
        {
            let coinB = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            let game_val = test_scenario::take_shared<Game>(scenario);
            let ctx = test_scenario::ctx(scenario);

            // check is player A is the correct one
            assert!(satoshi_flip::playerA(&game_val) == @0xBAE, EWrongPlayerA);
            //check the minimum bet
            assert!(satoshi_flip::min_bet(&game_val) == 100, EWrongMinBet);
            //check maximun bet
            assert!(satoshi_flip::max_bet(&game_val) == 1000, EWrongMaxBet);

            let guess = 0;
            
            // ready to place the bet
            satoshi_flip::bet(&mut game_val, guess, coinB, ctx);

            test_scenario::return_shared(game_val);
        };

        // player A reveals the secret and the game ends
        test_scenario::next_tx(scenario, playerA);
        {
            let game_val = test_scenario::take_shared<Game>(scenario);
            let ctx = test_scenario::ctx(scenario);
            let game = &mut game_val;

            satoshi_flip::end_game(game, secret, ctx);

            test_scenario::return_shared(game_val);
        };

        test_scenario::next_tx(scenario, playerA);
        {
            let game_val = test_scenario::take_shared<Game>(scenario);
            let game = &mut game_val;

            // check that player A has the correct amount
            let coinA = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            assert!(coin::value(&coinA) == 1300, EWrongPlayerATotal);
            test_scenario::return_to_sender(scenario, coinA);

            //check the game's outcome
            assert!(!satoshi_flip::is_playerB_winner(game), EWrongOutcome);
            assert!(satoshi_flip::secret(game) == vector[1,0,0,1,1,1,0,0,1,0,0,1,0,1], EWrongOutcome);
            assert!(satoshi_flip::guess(game) == 0, EWrongOutcome);

            test_scenario::return_shared(game_val);
        };
        test_scenario::end(scenario_val);
    }

    #[test]
    fun playerB_wins_test() {
        let world = @0x1EE7; // needed only for beginning the test_scenario
        let playerA = @0xBAE;
        let playerB = @0xFAB;
        let secret: vector<u8> = vector[1,0,0,1,1,1,0,0,1,0,0,1,0,1];
        let secret_hash = sha3_256(secret);
        let min_bet = 100;

        let scenario_val = test_scenario::begin(world);
        let scenario = &mut scenario_val;
        {
            init(test_scenario::ctx(scenario), playerA, playerB);
        };

        // player A creates the game
        test_scenario::next_tx(scenario, playerA);
        {
            let coinA = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            let ctx = test_scenario::ctx(scenario);
            satoshi_flip::start_game(secret_hash, coinA, min_bet, ctx);
        };

        // player B checks the game details and places a bet
        test_scenario::next_tx(scenario, playerB);
        {
            let coinB = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            let game_val = test_scenario::take_shared<Game>(scenario);
            let ctx = test_scenario::ctx(scenario);

            // check is player A is the correct one
            assert!(satoshi_flip::playerA(&game_val) == @0xBAE, EWrongPlayerA);
            //check the minimum bet
            assert!(satoshi_flip::min_bet(&game_val) == 100, EWrongMinBet);
            //check maximun bet
            assert!(satoshi_flip::max_bet(&game_val) == 1000, EWrongMaxBet);

            let guess = 1;
            
            // ready to place the bet
            satoshi_flip::bet(&mut game_val, guess, coinB, ctx);

            test_scenario::return_shared(game_val);
        };

        // player A reveals the secret and the game ends
        test_scenario::next_tx(scenario, playerA);
        {
            let game_val = test_scenario::take_shared<Game>(scenario);
            let ctx = test_scenario::ctx(scenario);
            let game = &mut game_val;

            satoshi_flip::end_game(game, secret, ctx);

            test_scenario::return_shared(game_val);
        };

        // check the game outcome is the one desired
        test_scenario::next_tx(scenario, playerB);
        {
            let game_val = test_scenario::take_shared<Game>(scenario);
            let game = &mut game_val;

            // check that player A has the correct amount
            let coinB = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            assert!(coin::value(&coinB) == 600, EWrongPlayerATotal);
            test_scenario::return_to_sender(scenario, coinB);

            //check the game's outcome
            assert!(satoshi_flip::is_playerB_winner(game), EWrongOutcome);
            assert!(satoshi_flip::secret(game) == vector[1,0,0,1,1,1,0,0,1,0,0,1,0,1], EWrongOutcome);
            assert!(satoshi_flip::guess(game) == 1, EWrongOutcome);

            test_scenario::return_shared(game_val);
        };

        // check playerA's balance
        test_scenario::next_tx(scenario, playerA);
        {
            // check that player A has the correct amount
            let coinA = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            assert!(coin::value(&coinA) == 700, EWrongPlayerATotal);
            test_scenario::return_to_sender(scenario, coinA);
        };
        test_scenario::end(scenario_val);
    }

}