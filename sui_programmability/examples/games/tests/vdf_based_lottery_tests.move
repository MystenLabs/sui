// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module games::vdf_based_lottery_tests {
    use sui::test_scenario::{Self};
    use games::vdf_based_lottery::{Self, Game, Ticket, GameWinner};

    #[test]
    #[expected_failure(abort_code = games::vdf_based_lottery::ESubmissionPhaseInProgress)]
    fun test_complete_too_early() {
        let user1 = @0x0;

        let scenario_val = test_scenario::begin(user1);
        let scenario = &mut scenario_val;

        let clock = sui::clock::create_for_testing(test_scenario::ctx(scenario));

        vdf_based_lottery::create(1000, 1000, &clock, test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, user1);
        let game_val = test_scenario::take_shared<Game>(scenario);
        let game = &mut game_val;

        // User1 buys a ticket.
        test_scenario::next_tx(scenario, user1);
        vdf_based_lottery::participate(game, b"user1 randomness", &clock, test_scenario::ctx(scenario));

        // Increment time but still in submission phase
        sui::clock::increment_for_testing(&mut clock, 500);

        // User1 tries to complete the lottery too early.
        test_scenario::next_tx(scenario, user1);
        vdf_based_lottery::complete(
            game,
            x"00407a69ae0ebf78ecdc5e022654d49d93b355bf6c27119caae8b4c2b1ca8e087070a448b88fb69f4c5c14ed83825c2474d081f1edfd2510e0b6ee7d59910b402ce00040ef37550d7fca0a88f9a6f29f416507b80dbe5a6b0171f4ffbc6eb6ac2e4b0cd52739aca2c31ec2f1846af7659a1df2cbcd63341da7458f065074ea2a5143d54f",
            x"00407930e28468f98c241876505183d2cc09f8f631d69e8e1b43b822c6044a2f2018d7ff2388191d155ddcf1a88408eba12c392ef8040016289a355fa621c22cfbbc00409d46ad1ac7cd056f324a6877beee586bb847d1080359b2a86a65771c48feb7e572625a63b99dd1592a64f0798c11d455eaf286ec715e4bb80edc9c7b5bc32d47",
            &clock
        );

        sui::clock::destroy_for_testing(clock);

        test_scenario::return_shared(game_val);
        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_play_vdf_lottery() {
        let user1 = @0x0;
        let user2 = @0x1;
        let user3 = @0x2;
        let user4 = @0x3;

        let scenario_val = test_scenario::begin(user1);
        let scenario = &mut scenario_val;

        let clock = sui::clock::create_for_testing(test_scenario::ctx(scenario));

        vdf_based_lottery::create(1000, 1000, &clock, test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, user1);
        let game_val = test_scenario::take_shared<Game>(scenario);
        let game = &mut game_val;

        // User1 buys a ticket.
        test_scenario::next_tx(scenario, user1);
        vdf_based_lottery::participate(game, b"user1 randomness", &clock, test_scenario::ctx(scenario));
        // User2 buys a ticket.
        test_scenario::next_tx(scenario, user2);
        vdf_based_lottery::participate(game, b"user2 randomness", &clock, test_scenario::ctx(scenario));
        // User3 buys a ticket
        test_scenario::next_tx(scenario, user3);
        vdf_based_lottery::participate(game, b"user3 randomness", &clock, test_scenario::ctx(scenario));
        // User4 buys a ticket
        test_scenario::next_tx(scenario, user4);
        vdf_based_lottery::participate(game, b"user4 randomness", &clock, test_scenario::ctx(scenario));

        // Increment time to after submission phase has ended
        sui::clock::increment_for_testing(&mut clock, 1000);

        // User3 completes by submitting output and proof of the VDF
        test_scenario::next_tx(scenario, user3);
        vdf_based_lottery::complete(
            game,
            x"00407a69ae0ebf78ecdc5e022654d49d93b355bf6c27119caae8b4c2b1ca8e087070a448b88fb69f4c5c14ed83825c2474d081f1edfd2510e0b6ee7d59910b402ce00040ef37550d7fca0a88f9a6f29f416507b80dbe5a6b0171f4ffbc6eb6ac2e4b0cd52739aca2c31ec2f1846af7659a1df2cbcd63341da7458f065074ea2a5143d54f",
            x"00407930e28468f98c241876505183d2cc09f8f631d69e8e1b43b822c6044a2f2018d7ff2388191d155ddcf1a88408eba12c392ef8040016289a355fa621c22cfbbc00409d46ad1ac7cd056f324a6877beee586bb847d1080359b2a86a65771c48feb7e572625a63b99dd1592a64f0798c11d455eaf286ec715e4bb80edc9c7b5bc32d47",
            &clock
        );

        // User1 is the winner since the mod of the hash results in 0.
        test_scenario::next_tx(scenario, user1);
        assert!(!test_scenario::has_most_recent_for_address<GameWinner>(user1), 1);
        let ticket = test_scenario::take_from_address<Ticket>(scenario, user1);
        let ticket_game_id = *vdf_based_lottery::get_ticket_game_id(&ticket);
        vdf_based_lottery::redeem(&ticket, &game_val, test_scenario::ctx(scenario));
        vdf_based_lottery::delete_ticket(ticket);

        // Make sure User1 now has a winner ticket for the right game id.
        test_scenario::next_tx(scenario, user1);
        let ticket = test_scenario::take_from_address<GameWinner>(scenario, user1);
        assert!(vdf_based_lottery::get_game_winner_game_id(&ticket) == &ticket_game_id, 1);
        test_scenario::return_to_address(user1, ticket);

        test_scenario::return_shared(game_val);
        test_scenario::end(scenario_val);
        sui::clock::destroy_for_testing(clock);
    }

    #[test]
    #[expected_failure(abort_code = games::vdf_based_lottery::EInvalidVdfOutput)]
    fun test_invalid_vdf_output() {
        let user1 = @0x0;
        let user2 = @0x1;
        let user3 = @0x2;
        let user4 = @0x3;

        let scenario_val = test_scenario::begin(user1);
        let scenario = &mut scenario_val;

        let clock = sui::clock::create_for_testing(test_scenario::ctx(scenario));

        vdf_based_lottery::create(1000, 1000, &clock, test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, user1);
        let game_val = test_scenario::take_shared<Game>(scenario);
        let game = &mut game_val;

        // User1 buys a ticket.
        test_scenario::next_tx(scenario, user1);
        vdf_based_lottery::participate(game, b"user1 randomness", &clock, test_scenario::ctx(scenario));
        // User2 buys a ticket.
        test_scenario::next_tx(scenario, user2);
        vdf_based_lottery::participate(game, b"user2 randomness", &clock, test_scenario::ctx(scenario));
        // User3 buys a ticket
        test_scenario::next_tx(scenario, user3);
        vdf_based_lottery::participate(game, b"user3 randomness", &clock, test_scenario::ctx(scenario));
        // User4 buys a ticket
        test_scenario::next_tx(scenario, user4);
        vdf_based_lottery::participate(game, b"user4 randomness", &clock, test_scenario::ctx(scenario));

        // Increment time to after submission phase has ended
        sui::clock::increment_for_testing(&mut clock, 1000);

        // User3 completes by submitting output and proof of the VDF
        test_scenario::next_tx(scenario, user3);
        vdf_based_lottery::complete(
            game,
            x"00406d56f20f7e53919752707ffd7b0ac5e4f8bed8f422a1d9f6d8a30d7cc890b9135a7da278f3d0d31005ae99b171237ac1774fe6b1a8debecc3f791495eb1ad2bc00403a6b2d08760b14887f58e3cb041400d2b85a4cfd39d85986deb78715fc253538118ca789924c8971bbd96a2da31206b2dd9849e2ed53a15d9b8e4c9a17be4587",
            x"004045551debe5f28c75dc3054dfd6f6467767023dfea96d1d23245564279026161182db7c3c608fc77b07aeb2c332b520503e28bc3c41aa0d1ea211aaa8922a82ec00402628d0d5d92f154531f59ed649e7958237c99424450c08e4ad07c4e3055bac20e09903026b3b16e68cafd8387ff61b978ce5e4179aafa68f83f357dc5193424f",
            &clock
        );

        sui::clock::destroy_for_testing(clock);

        test_scenario::return_shared(game_val);
        test_scenario::end(scenario_val);
    }
}
