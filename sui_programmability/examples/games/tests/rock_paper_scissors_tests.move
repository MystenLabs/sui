// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module games::rock_paper_scissors_tests {
    use games::rock_paper_scissors::{Self as game, Game, PlayerTurn, Secret, ThePrize};
    use sui::test_scenario::{Self};
    use std::hash;

    #[test]
    fun play_rock_paper_scissors() {
        // So these are our heroes.
        let the_main_guy = @0xA1C05;
        let mr_lizard = @0xA55555;
        let mr_spock = @0x590C;

        let mut scenario = test_scenario::begin(the_main_guy);

        // Let the game begin!
        game::new_game(mr_spock, mr_lizard, scenario.ctx());

        // Mr Spock makes his move. He does it secretly and hashes the gesture with a salt
        // so that only he knows what it is.
        scenario.next_tx(mr_spock);
        {
            let hash = hash(game::rock(), b"my_phaser_never_failed_me!");
            game::player_turn(the_main_guy, hash, scenario.ctx());
        };

        // Now it's time for The Main Guy to accept his turn.
        scenario.next_tx(the_main_guy);
        {
            let mut game = scenario.take_from_sender<Game>();
            let cap = scenario.take_from_sender<PlayerTurn>();

            assert!(game.status() == 0, 0); // STATUS_READY

            game.add_hash(cap);

            assert!(game.status() == 1, 0); // STATUS_HASH_SUBMISSION

            scenario.return_to_sender(game);
        };

        // Same for Mr Lizard. He uses his secret phrase to encode his turn.
        scenario.next_tx(mr_lizard);
        {
            let hash = hash(game::scissors(), b"sssssss_you_are_dead!");
            game::player_turn(the_main_guy, hash, scenario.ctx());
        };

        scenario.next_tx(the_main_guy);
        {
            let mut game = scenario.take_from_sender<Game>();
            let cap = scenario.take_from_sender<PlayerTurn>();
            game.add_hash(cap);

            assert!(game.status() == 2, 0); // STATUS_HASHES_SUBMITTED

            scenario.return_to_sender(game);
        };

        // Now that both sides made their moves, it's time for  Mr Spock and Mr Lizard to
        // reveal their secrets. The Main Guy will then be able to determine the winner. Who's
        // gonna win The Prize? We'll see in a bit!
        scenario.next_tx(mr_spock);
        game::reveal(the_main_guy, b"my_phaser_never_failed_me!", scenario.ctx());

        scenario.next_tx(the_main_guy);
        {
            let mut game = scenario.take_from_sender<Game>();
            let secret = scenario.take_from_sender<Secret>();
            game.match_secret(secret);

            assert!(game.status() == 3, 0); // STATUS_REVEALING

            scenario.return_to_sender(game);
        };

        scenario.next_tx(mr_lizard);
        game::reveal(the_main_guy, b"sssssss_you_are_dead!", scenario.ctx());

        // The final step. The Main Guy matches and reveals the secret of the Mr Lizard and
        // calls the [`select_winner`] function to release The Prize.
        scenario.next_tx(the_main_guy);
        {
            let mut game = scenario.take_from_sender<Game>();
            let secret = scenario.take_from_sender<Secret>();
            game.match_secret(secret);

            assert!(game.status() == 4, 0); // STATUS_REVEALED

            game.select_winner(scenario.ctx());
        };

        scenario.next_tx(mr_spock);
        // If it works, then MrSpock is in possession of the prize;
        let prize = scenario.take_from_sender<ThePrize>();
        // Don't forget to give it back!
        scenario.return_to_sender(prize);
        scenario.end();
    }

    // Copy of the hashing function from the main module.
    fun hash(gesture: u8, mut salt: vector<u8>): vector<u8> {
        salt.push_back(gesture);
        hash::sha2_256(salt)
    }
}
