// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module games::rock_paper_scissors_tests {
    use games::rock_paper_scissors::{Self as Game, Game, PlayerTurn, Secret, ThePrize};
    use sui::test_scenario::{Self};
    use std::vector;
    use std::hash;

    #[test]
    fun play_rock_paper_scissors() {
        // So these are our heroes.
        let the_main_guy = @0xA1C05;
        let mr_lizard = @0xA55555;
        let mr_spock = @0x590C;

        let scenario_val = test_scenario::begin(the_main_guy);
        let scenario = &mut scenario_val;

        // Let the game begin!
        Game::new_game(mr_spock, mr_lizard, test_scenario::ctx(scenario));

        // Mr Spock makes his move. He does it secretly and hashes the gesture with a salt
        // so that only he knows what it is.
        test_scenario::next_tx(scenario, mr_spock);
        {
            let hash = hash(Game::rock(), b"my_phaser_never_failed_me!");
            Game::player_turn(the_main_guy, hash, test_scenario::ctx(scenario));
        };

        // Now it's time for The Main Guy to accept his turn.
        test_scenario::next_tx(scenario, the_main_guy);
        {
            let game = test_scenario::take_from_sender<Game>(scenario);
            let cap = test_scenario::take_from_sender<PlayerTurn>(scenario);

            assert!(Game::status(&game) == 0, 0); // STATUS_READY

            Game::add_hash(&mut game, cap);

            assert!(Game::status(&game) == 1, 0); // STATUS_HASH_SUBMISSION

            test_scenario::return_to_sender(scenario, game);
        };

        // Same for Mr Lizard. He uses his secret phrase to encode his turn.
        test_scenario::next_tx(scenario, mr_lizard);
        {
            let hash = hash(Game::scissors(), b"sssssss_you_are_dead!");
            Game::player_turn(the_main_guy, hash, test_scenario::ctx(scenario));
        };

        test_scenario::next_tx(scenario, the_main_guy);
        {
            let game = test_scenario::take_from_sender<Game>(scenario);
            let cap = test_scenario::take_from_sender<PlayerTurn>(scenario);
            Game::add_hash(&mut game, cap);

            assert!(Game::status(&game) == 2, 0); // STATUS_HASHES_SUBMITTED

            test_scenario::return_to_sender(scenario, game);
        };

        // Now that both sides made their moves, it's time for  Mr Spock and Mr Lizard to
        // reveal their secrets. The Main Guy will then be able to determine the winner. Who's
        // gonna win The Prize? We'll see in a bit!
        test_scenario::next_tx(scenario, mr_spock);
        Game::reveal(the_main_guy, b"my_phaser_never_failed_me!", test_scenario::ctx(scenario));

        test_scenario::next_tx(scenario, the_main_guy);
        {
            let game = test_scenario::take_from_sender<Game>(scenario);
            let secret = test_scenario::take_from_sender<Secret>(scenario);
            Game::match_secret(&mut game, secret);

            assert!(Game::status(&game) == 3, 0); // STATUS_REVEALING

            test_scenario::return_to_sender(scenario, game);
        };

        test_scenario::next_tx(scenario, mr_lizard);
        Game::reveal(the_main_guy, b"sssssss_you_are_dead!", test_scenario::ctx(scenario));

        // The final step. The Main Guy matches and reveals the secret of the Mr Lizard and
        // calls the [`select_winner`] function to release The Prize.
        test_scenario::next_tx(scenario, the_main_guy);
        {
            let game = test_scenario::take_from_sender<Game>(scenario);
            let secret = test_scenario::take_from_sender<Secret>(scenario);
            Game::match_secret(&mut game, secret);

            assert!(Game::status(&game) == 4, 0); // STATUS_REVEALED

            Game::select_winner(game, test_scenario::ctx(scenario));
        };

        test_scenario::next_tx(scenario, mr_spock);
        // If it works, then MrSpock is in possession of the prize;
        let prize = test_scenario::take_from_sender<ThePrize>(scenario);
        // Don't forget to give it back!
        test_scenario::return_to_sender(scenario, prize);
        test_scenario::end(scenario_val);
    }

    // Copy of the hashing function from the main module.
    fun hash(gesture: u8, salt: vector<u8>): vector<u8> {
        vector::push_back(&mut salt, gesture);
        hash::sha2_256(salt)
    }
}
