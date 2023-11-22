// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module games::drand_based_lottery_tests {
    use sui::test_scenario::{Self};
    use games::drand_based_lottery::{Self, Game, Ticket, GameWinner};
    use games::drand_lib::verify_time_has_passed;

    #[test]
    fun test_verify_time_has_passed_success() {
        // Taken from the output of
        // curl https://drand.cloudflare.com/52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971/public/8
        verify_time_has_passed(
            1692803367 + 3*7, // exactly the 8th round
            x"a0c06b9964123d2e6036aa004c140fc301f4edd3ea6b8396a15dfd7dfd70cc0dce0b4a97245995767ab72cf59de58c47",
            8
        );
        verify_time_has_passed(
            1692803367 + 3*7 - 2, // the 8th round - 2 seconds
            x"a0c06b9964123d2e6036aa004c140fc301f4edd3ea6b8396a15dfd7dfd70cc0dce0b4a97245995767ab72cf59de58c47",
            8
        );
    }

    #[test]
    #[expected_failure(abort_code = games::drand_lib::EInvalidProof)]
    fun test_verify_time_has_passed_failure() {
        // Taken from the output of
        // curl https://drand.cloudflare.com/52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971/public/8
        verify_time_has_passed(
            1692803367 + 3*8, // exactly the 9th round - 10 seconds
            x"a0c06b9964123d2e6036aa004c140fc301f4edd3ea6b8396a15dfd7dfd70cc0dce0b4a97245995767ab72cf59de58c47",
            8
        );
    }

    #[test]
    fun test_play_drand_lottery() {
        let user1 = @0x0;
        let user2 = @0x1;
        let user3 = @0x2;
        let user4 = @0x3;

        let scenario_val = test_scenario::begin(user1);
        let scenario = &mut scenario_val;

        drand_based_lottery::create(10, test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, user1);
        let game_val = test_scenario::take_shared<Game>(scenario);
        let game = &mut game_val;

        // User1 buys a ticket.
        test_scenario::next_tx(scenario, user1);
        drand_based_lottery::participate(game, test_scenario::ctx(scenario));
        // User2 buys a ticket.
        test_scenario::next_tx(scenario, user2);
        drand_based_lottery::participate(game, test_scenario::ctx(scenario));
        // User3 buys a tcket
        test_scenario::next_tx(scenario, user3);
        drand_based_lottery::participate(game, test_scenario::ctx(scenario));
        // User4 buys a tcket
        test_scenario::next_tx(scenario, user4);
        drand_based_lottery::participate(game, test_scenario::ctx(scenario));

        // User 2 closes the game.
        test_scenario::next_tx(scenario, user2);
        // Taken from the output of
        // curl https://drand.cloudflare.com/52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971/public/8
        drand_based_lottery::close(
            game,
            x"a0c06b9964123d2e6036aa004c140fc301f4edd3ea6b8396a15dfd7dfd70cc0dce0b4a97245995767ab72cf59de58c47",
        );

        // User3 completes the game.
        test_scenario::next_tx(scenario, user3);
        // Taken from theoutput of
        // curl https://drand.cloudflare.com/52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971/public/10
        drand_based_lottery::complete(
            game,
            x"ac415e508c484053efed1c6c330e3ae0bf20185b66ed088864dac1ff7d6f927610824986390d3239dac4dd73e6f865f5",
        );

        // User2 is the winner since the mod of the hash results in 1.
        test_scenario::next_tx(scenario, user2);
        assert!(!test_scenario::has_most_recent_for_address<GameWinner>(user2), 1);
        let ticket = test_scenario::take_from_address<Ticket>(scenario, user2);
        let ticket_game_id = *drand_based_lottery::get_ticket_game_id(&ticket);
        drand_based_lottery::redeem(&ticket, &game_val, test_scenario::ctx(scenario));
        drand_based_lottery::delete_ticket(ticket);

        // Make sure User2 now has a winner ticket for the right game id.
        test_scenario::next_tx(scenario, user2);
        let ticket = test_scenario::take_from_address<GameWinner>(scenario, user2);
        assert!(drand_based_lottery::get_game_winner_game_id(&ticket) == &ticket_game_id, 1);
        test_scenario::return_to_address(user2, ticket);

        test_scenario::return_shared(game_val);
        test_scenario::end(scenario_val);
    }
}
