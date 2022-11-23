// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module games_with_chance::test_satoshi_flip {
    // imports
    use std::hash::sha3_256;

    use sui::coin::{Self, Coin};
    use sui::sui::SUI;
    use sui::transfer;
    use sui::test_scenario;
    use sui::tx_context::TxContext;

    use games_with_chance::satoshi_flip::{Self, Game};


    const EWronghouse: u64 = 0;
    const EWrongMinBet: u64 = 1;
    const EWrongMaxBet: u64 = 2;
    const EWronghouseTotal: u64 = 3;
    const EWrongOutcome: u64 = 4;
    const EWrongPlayerTotal: u64 = 5;


    fun start(ctx: &mut TxContext, house: address, player: address) {
        // send coins to players
        let coinA = coin::mint_for_testing<SUI>(50000, ctx);
        let coinB = coin::mint_for_testing<SUI>(5000, ctx);
        transfer::transfer(coinA, house);
        transfer::transfer(coinB, player);
    }

    #[test]
    fun house_wins_test() {
        let world = @0x1EE7; // needed only for beginning the test_scenario.
        let house = @0xBAE;
        let player = @0xFAB;
        // for testing purposes we use a weak secret, in practice this should be random and at least 16 bytes long.
        let secret = b"supersecret";
        let secret_hash = sha3_256(secret);
        let min_bet = 100;
        let max_bet = 5000;

        let scenario_val = test_scenario::begin(world);
        let scenario = &mut scenario_val;
        {
            start(test_scenario::ctx(scenario), house, player);
        };

        // house creates the game.
        test_scenario::next_tx(scenario, house);
        {
            let coinA = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            let ctx = test_scenario::ctx(scenario);
            satoshi_flip::start_game(secret_hash, coinA, min_bet, max_bet, ctx);
        };

        // player checks the game details and places a bet.
        test_scenario::next_tx(scenario, player);
        {
            let coinB = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            let game_val = test_scenario::take_shared<Game>(scenario);
            let ctx = test_scenario::ctx(scenario);

            // check is house address the correct one.
            assert!(satoshi_flip::house(&game_val) == @0xBAE, EWronghouse);
            // check the minimum bet.
            assert!(satoshi_flip::min_bet(&game_val) == 100, EWrongMinBet);
            // check maximun bet.
            assert!(satoshi_flip::max_bet(&game_val) == 5000, EWrongMaxBet);

            let guess = 0;

            // ready to place the bet.
            satoshi_flip::bet(&mut game_val, guess, coinB,  ctx);

            test_scenario::return_shared(game_val);
        };

        // house reveals the secret and the game ends.
        test_scenario::next_tx(scenario, house);
        {
            let game_val = test_scenario::take_shared<Game>(scenario);
            let ctx = test_scenario::ctx(scenario);
            let game = &mut game_val;

            satoshi_flip::end_game(game, secret, ctx);

            test_scenario::return_shared(game_val);
        };

        test_scenario::next_tx(scenario, house);
        {
            let game_val = test_scenario::take_shared<Game>(scenario);
            let game = &mut game_val;

            // check that house has the correct amount
            let coinA = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            assert!(coin::value(&coinA) == 55000, EWronghouseTotal);
            test_scenario::return_to_sender(scenario, coinA);

            //check the game's outcome
            assert!(!satoshi_flip::is_player_winner(game), EWrongOutcome);
            assert!(satoshi_flip::secret(game) == b"supersecret", EWrongOutcome);
            assert!(satoshi_flip::guess(game) == 0, EWrongOutcome);

            test_scenario::return_shared(game_val);
        };
        test_scenario::end(scenario_val);
    }

    #[test]
    fun player_wins_test() {
        let world = @0x1EE7; // needed only for beginning the test_scenario.
        let house = @0xBAE;
        let player = @0xFAB;
        let secret = b"supersecret";
        let secret_hash = sha3_256(secret);
        let max_bet = 5000;
        let min_bet = 100;

        let scenario_val = test_scenario::begin(world);
        let scenario = &mut scenario_val;
        {
            start(test_scenario::ctx(scenario), house, player);
        };

        // house creates the game.
        test_scenario::next_tx(scenario, house);
        {
            let coinA = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            let ctx = test_scenario::ctx(scenario);
            satoshi_flip::start_game(secret_hash, coinA, min_bet, max_bet, ctx);
        };

        // player checks the game details and places a bet.
        test_scenario::next_tx(scenario, player);
        {
            let coinB = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            let game_val = test_scenario::take_shared<Game>(scenario);
            let ctx = test_scenario::ctx(scenario);

            // check is house address the correct one.
            assert!(satoshi_flip::house(&game_val) == @0xBAE, EWronghouse);
            //check the minimum bet.
            assert!(satoshi_flip::min_bet(&game_val) == 100, EWrongMinBet);
            //check maximun bet.
            assert!(satoshi_flip::max_bet(&game_val) == 5000, EWrongMaxBet);

            let guess = 1;

            satoshi_flip::bet(&mut game_val, guess, coinB, ctx);

            test_scenario::return_shared(game_val);
        };

        // house reveals the secret and the game ends.
        test_scenario::next_tx(scenario, house);
        {
            let game_val = test_scenario::take_shared<Game>(scenario);
            let ctx = test_scenario::ctx(scenario);
            let game = &mut game_val;

            satoshi_flip::end_game(game, secret, ctx);

            test_scenario::return_shared(game_val);
        };

        // check the game outcome is the one desired.
        test_scenario::next_tx(scenario, player);
        {
            let game_val = test_scenario::take_shared<Game>(scenario);
            let game = &mut game_val;

            // check that player has the correct amount.
            let coinB = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            assert!(coin::value(&coinB) == 10000, EWronghouseTotal);
            test_scenario::return_to_sender(scenario, coinB);

            //check the game's outcome.
            assert!(satoshi_flip::is_player_winner(game), EWrongOutcome);
            assert!(satoshi_flip::secret(game) == b"supersecret", EWrongOutcome);
            assert!(satoshi_flip::guess(game) == 1, EWrongOutcome);

            test_scenario::return_shared(game_val);
        };

        // check house's balance.
        test_scenario::next_tx(scenario, house);
        {
            // check that house has the correct amount.
            let coinA = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            assert!(coin::value(&coinA) == 45000, EWronghouseTotal);
            test_scenario::return_to_sender(scenario, coinA);
        };
        test_scenario::end(scenario_val);
    }

    // house cancel's with wrong secret (forgotten) before bet.
    #[test]
    fun house_ends_before_bet() {
        let world = @0x1EE7; // needed only for beginning the test_scenario.
        let house = @0xBAE;
        let player = @0xFAB;
        let secret = b"supersecret";
        let secret_hash = sha3_256(secret);
        let min_bet = 100;
        let max_bet = 5000;

        let scenario_val = test_scenario::begin(world);
        let scenario = &mut scenario_val;
        {
            start(test_scenario::ctx(scenario), house, player);
        };

        // house creates the game.
        test_scenario::next_tx(scenario, house);
        {
            let coinA = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            let ctx = test_scenario::ctx(scenario);
            satoshi_flip::start_game(secret_hash, coinA, min_bet, max_bet, ctx);
        };

        // house ends game.
        test_scenario::next_tx(scenario, house);
        {
            let game_val = test_scenario::take_shared<Game>(scenario);
            let game = &mut game_val;
            let ctx = test_scenario::ctx(scenario);

            satoshi_flip::end_game(game, secret, ctx);

            test_scenario::return_shared(game_val);
        };

        // check house's balance.
        test_scenario::next_tx(scenario, house);
        {
            // check that house has the correct amount.
            let coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            assert!(coin::value(&coin) == 50000, EWronghouseTotal);
            test_scenario::return_to_sender(scenario, coin);
        };

        test_scenario::end(scenario_val);
    }


    // Tests expecting abort.

    // tests for start_game with wrong inputs.
    // Check that min_bet <= max_bet is enforced properly.
    #[test]
    #[expected_failure(abort_code = 4)]
    fun house_wrong_min_max_bet() {
        let world = @0x1EE7; // needed only for beginning the test_scenario.
        let house = @0xBAE;
        let player = @0xFAB;
        let secret = b"supersecret";
        let secret_hash = sha3_256(secret);
        let max_bet = 5000;
        let min_bet = 10000; // this is too high here.

        let scenario_val = test_scenario::begin(world);
        let scenario = &mut scenario_val;
        {
            start(test_scenario::ctx(scenario), house, player);
        };

        // house creates the game.
        test_scenario::next_tx(scenario, house);
        {
            let coinA = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            let ctx = test_scenario::ctx(scenario);
            satoshi_flip::start_game(secret_hash, coinA, min_bet, max_bet, ctx);
        };
        test_scenario::end(scenario_val);
    }

    // Check that house provided coin of sufficient amount to cover the max_bet.
    #[test]
    #[expected_failure(abort_code = 9)]
    fun house_insufficient_balance() {
        let world = @0x1EE7; // needed only for beginning the test_scenario.
        let house = @0xBAE;
        let player = @0xFAB;
        let secret = b"supersecret";
        let secret_hash = sha3_256(secret);
        let max_bet = 60000; // House provides a 50000 Mist Coin.
        let min_bet = 10000;

        let scenario_val = test_scenario::begin(world);
        let scenario = &mut scenario_val;
        {
            start(test_scenario::ctx(scenario), house, player);
        };

        // house creates the game.
        test_scenario::next_tx(scenario, house);
        {
            let coinA = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            let ctx = test_scenario::ctx(scenario);
            satoshi_flip::start_game(secret_hash, coinA, min_bet, max_bet, ctx);
        };
        test_scenario::end(scenario_val);
    }

    // Test house setting min_bet = 0

    #[test]
    #[expected_failure(abort_code = 10)]
    fun house_sets_min_bet_0() {
        let world = @0x1EE7; // needed only for beginning the test_scenario.
        let house = @0xBAE;
        let player = @0xFAB;
        let secret = b"supersecret";
        let secret_hash = sha3_256(secret);
        let max_bet = 5000;
        let min_bet = 0;

        let scenario_val = test_scenario::begin(world);
        let scenario = &mut scenario_val;
        {
            start(test_scenario::ctx(scenario), house, player);
        };

        // house creates the game.
        test_scenario::next_tx(scenario, house);
        {
            let coinA = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            let ctx = test_scenario::ctx(scenario);
            satoshi_flip::start_game(secret_hash, coinA, min_bet, max_bet, ctx);
        };
        test_scenario::end(scenario_val);
    }

    // tests for bet function with wrong data.

    // player stake too high.
    #[test]
    #[expected_failure(abort_code = 0)]
    fun player_stake_exceeds_max_bet() {
        let world = @0x1EE7; // needed only for beginning the test_scenario.
        let house = @0xBAE;
        let player = @0xFAB;
        let secret = b"supersecret";
        let secret_hash = sha3_256(secret);
        let max_bet = 4999;
        let min_bet = 1000;

        let scenario_val = test_scenario::begin(world);
        let scenario = &mut scenario_val;
        {
            start(test_scenario::ctx(scenario), house, player);
        };

        // house creates the game.
        test_scenario::next_tx(scenario, house);
        {
            let coinA = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            let ctx = test_scenario::ctx(scenario);
            satoshi_flip::start_game(secret_hash, coinA, min_bet, max_bet, ctx);
        };

        // player's stake is too high.
        test_scenario::next_tx(scenario, player);
        {
            let coinB = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            let game_val = test_scenario::take_shared<Game>(scenario);
            let ctx = test_scenario::ctx(scenario);

            let guess = 0;

            // ready to place the bet.
            satoshi_flip::bet(&mut game_val, guess, coinB,  ctx);

            test_scenario::return_shared(game_val);
        };
        test_scenario::end(scenario_val);
    }

    // player stake too low.
    #[test]
    #[expected_failure(abort_code = 1)]
    fun player_stake_bellow_min_bet() {
        let world = @0x1EE7; // needed only for beginning the test_scenario.
        let house = @0xBAE;
        let player = @0xFAB;
        let secret = b"supersecret";
        let secret_hash = sha3_256(secret);
        let max_bet = 10000;
        let min_bet = 5001;

        let scenario_val = test_scenario::begin(world);
        let scenario = &mut scenario_val;
        {
            start(test_scenario::ctx(scenario), house, player);
        };

        // house creates the game.
        test_scenario::next_tx(scenario, house);
        {
            let coinA = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            let ctx = test_scenario::ctx(scenario);
            satoshi_flip::start_game(secret_hash, coinA, min_bet, max_bet, ctx);
        };

        // player's stake is too low.
        test_scenario::next_tx(scenario, player);
        {
            let coinB = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            let game_val = test_scenario::take_shared<Game>(scenario);
            let ctx = test_scenario::ctx(scenario);

            let guess = 0;

            // ready to place the bet.
            satoshi_flip::bet(&mut game_val, guess, coinB,  ctx);

            test_scenario::return_shared(game_val);
        };
        test_scenario::end(scenario_val);
    }

    // player's guess is not 1 or 0.
    #[test]
    #[expected_failure(abort_code = 2)]
    fun player_wrong_guess() {
        let world = @0x1EE7; // needed only for beginning the test_scenario.
        let house = @0xBAE;
        let player = @0xFAB;
        let secret = b"supersecret";
        let secret_hash = sha3_256(secret);
        let max_bet = 5000;
        let min_bet = 1000;

        let scenario_val = test_scenario::begin(world);
        let scenario = &mut scenario_val;
        {
            start(test_scenario::ctx(scenario), house, player);
        };

        // house creates the game.
        test_scenario::next_tx(scenario, house);
        {
            let coinA = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            let ctx = test_scenario::ctx(scenario);
            satoshi_flip::start_game(secret_hash, coinA, min_bet, max_bet, ctx);
        };

        test_scenario::next_tx(scenario, player);
        {
            let coinB = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            let game_val = test_scenario::take_shared<Game>(scenario);
            let ctx = test_scenario::ctx(scenario);

            let guess = 5;
            satoshi_flip::bet(&mut game_val, guess, coinB,  ctx);

            test_scenario::return_shared(game_val);
        };
        test_scenario::end(scenario_val);
    }

    // tests for wrong inputs in end_game.

    // test wrong address calling end_game.
    #[test]
    #[expected_failure(abort_code = 7)]
    fun random_player_calls_end_game() {
        let world = @0x1EE7; // needed only for beginning the test_scenario.
        let house = @0xBAE;
        let player = @0xFAB;
        let random_player = @0xCAFE;
        let secret = b"supersecret";
        let secret_hash = sha3_256(secret);
        let max_bet = 40000; 
        let min_bet = 1000;

        let scenario_val = test_scenario::begin(world);
        let scenario = &mut scenario_val;
        {
            start(test_scenario::ctx(scenario), house, player);
        };

        // house creates the game.
        test_scenario::next_tx(scenario, house);
        {
            let coinA = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            let ctx = test_scenario::ctx(scenario);
            satoshi_flip::start_game(secret_hash, coinA, min_bet, max_bet, ctx);
        };

        test_scenario::next_tx(scenario, player);
        {
            let coinB = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            let game_val = test_scenario::take_shared<Game>(scenario);
            let ctx = test_scenario::ctx(scenario);

            let guess = 0;

            // ready to place the bet.
            satoshi_flip::bet(&mut game_val, guess, coinB,  ctx);

            test_scenario::return_shared(game_val);
        };

        test_scenario::next_tx(scenario, random_player);
        {
            let game_val = test_scenario::take_shared<Game>(scenario);
            let ctx = test_scenario::ctx(scenario);
            let game = &mut game_val;

            satoshi_flip::end_game(game, secret, ctx);

            test_scenario::return_shared(game_val);
        };
        test_scenario::end(scenario_val);
    }

    // test wrong secret.
    #[test]
    #[expected_failure(abort_code = 3)]
    fun end_game_wrong_secret() {
        let world = @0x1EE7; // needed only for beginning the test_scenario.
        let house = @0xBAE;
        let player = @0xFAB;
        let secret = b"supersecret";
        let wrong_secret = b"simplesecret";
        let secret_hash = sha3_256(secret);
        let max_bet = 40000; 
        let min_bet = 1000;

        let scenario_val = test_scenario::begin(world);
        let scenario = &mut scenario_val;
        {
            start(test_scenario::ctx(scenario), house, player);
        };

        // house creates the game.
        test_scenario::next_tx(scenario, house);
        {
            let coinA = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            let ctx = test_scenario::ctx(scenario);
            satoshi_flip::start_game(secret_hash, coinA, min_bet, max_bet, ctx);
        };

        test_scenario::next_tx(scenario, player);
        {
            let coinB = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            let game_val = test_scenario::take_shared<Game>(scenario);
            let ctx = test_scenario::ctx(scenario);

            let guess = 0;

            satoshi_flip::bet(&mut game_val, guess, coinB,  ctx);

            test_scenario::return_shared(game_val);
        };

        test_scenario::next_tx(scenario, house);
        {
            let game_val = test_scenario::take_shared<Game>(scenario);
            let ctx = test_scenario::ctx(scenario);
            let game = &mut game_val;

            satoshi_flip::end_game(game, wrong_secret, ctx);

            test_scenario::return_shared(game_val);
        };

        test_scenario::end(scenario_val);
    }

    // cancel_game failures.

    // cancel_game before bet.
    #[test]
    #[expected_failure(abort_code = 8)]
    fun call_cancel_game_before_bet() {
        let world = @0x1EE7; // needed only for beginning the test_scenario.
        let house = @0xBAE;
        let player = @0xFAB;
        let secret = b"supersecret";
        let secret_hash = sha3_256(secret);
        let max_bet = 5000;
        let min_bet = 1000;

        let scenario_val = test_scenario::begin(world);
        let scenario = &mut scenario_val;
        {
            start(test_scenario::ctx(scenario), house, player);
        };

        // house creates the game.
        test_scenario::next_tx(scenario, house);
        {
            let coinA = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            let ctx = test_scenario::ctx(scenario);
            satoshi_flip::start_game(secret_hash, coinA, min_bet, max_bet, ctx);
        };

        test_scenario::next_tx(scenario, player);
        {
            let game_val = test_scenario::take_shared<Game>(scenario);
            let ctx = test_scenario::ctx(scenario);
            let game = &mut game_val;

            satoshi_flip::cancel_game(game, ctx);

            test_scenario::return_shared(game_val);
        };
        test_scenario::end(scenario_val);
    }

    // house wins and player tries to cancel.
    #[test]
    #[expected_failure(abort_code = 5)]
    fun player_cancel_after_end() {
        let world = @0x1EE7; // needed only for beginning the test_scenario.
        let house = @0xBAE;
        let player = @0xFAB;
        let secret = b"supersecret";
        let secret_hash = sha3_256(secret);
        let min_bet = 100;
        let max_bet = 5000;

        let scenario_val = test_scenario::begin(world);
        let scenario = &mut scenario_val;
        {
            start(test_scenario::ctx(scenario), house, player);
        };

        // house creates the game.
        test_scenario::next_tx(scenario, house);
        {
            let coinA = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            let ctx = test_scenario::ctx(scenario);
            satoshi_flip::start_game(secret_hash, coinA, min_bet, max_bet, ctx);
        };

        test_scenario::next_tx(scenario, player);
        {
            let coinB = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            let game_val = test_scenario::take_shared<Game>(scenario);
            let ctx = test_scenario::ctx(scenario);

            let guess = 0;

            // ready to place the bet.
            satoshi_flip::bet(&mut game_val, guess, coinB,  ctx);

            test_scenario::return_shared(game_val);
        };

        // house reveals the secret and the game ends.
        test_scenario::next_tx(scenario, house);
        {
            let game_val = test_scenario::take_shared<Game>(scenario);
            let ctx = test_scenario::ctx(scenario);
            let game = &mut game_val;

            satoshi_flip::end_game(game, secret, ctx);

            test_scenario::return_shared(game_val);
        };

        // player tries to cancel.
        test_scenario::next_tx(scenario, player);
        {
            let game_val = test_scenario::take_shared<Game>(scenario);
            let ctx = test_scenario::ctx(scenario);
            let game = &mut game_val;

            satoshi_flip::cancel_game(game, ctx);

            test_scenario::return_shared(game_val);
        };
        test_scenario::end(scenario_val);
    }
    
    /*
        stuff unable to be checked:
        - The player can only cancel after the required number of epochs passed, provided that the game hasn't ended (house refuses to call end_game)
    */


}