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

        let mut scenario = test_scenario::begin(user1);

        drand_based_lottery::create(10, scenario.ctx());
        scenario.next_tx(user1);
        let mut game_val = scenario.take_shared<Game>();
        let game = &mut game_val;

        // User1 buys a ticket.
        scenario.next_tx(user1);
        game.participate(scenario.ctx());
        // User2 buys a ticket.
        scenario.next_tx(user2);
        game.participate(scenario.ctx());
        // User3 buys a tcket
        scenario.next_tx(user3);
        game.participate(scenario.ctx());
        // User4 buys a tcket
        scenario.next_tx(user4);
        game.participate(scenario.ctx());

        // User 2 closes the game.
        scenario.next_tx(user2);
        // Taken from the output of
        // curl https://drand.cloudflare.com/52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971/public/8
        game.close(
            x"a0c06b9964123d2e6036aa004c140fc301f4edd3ea6b8396a15dfd7dfd70cc0dce0b4a97245995767ab72cf59de58c47",
        );

        // User3 completes the game.
        scenario.next_tx(user3);
        // Taken from theoutput of
        // curl https://drand.cloudflare.com/52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971/public/10
        game.complete(
            x"ac415e508c484053efed1c6c330e3ae0bf20185b66ed088864dac1ff7d6f927610824986390d3239dac4dd73e6f865f5",
        );

        // User2 is the winner since the mod of the hash results in 1.
        scenario.next_tx(user2);
        assert!(!test_scenario::has_most_recent_for_address<GameWinner>(user2), 1);
        let ticket = scenario.take_from_address<Ticket>(user2);
        let ticket_game_id = *ticket.get_ticket_game_id();
        ticket.redeem(&game_val, scenario.ctx());
        ticket.delete_ticket();

        // Make sure User2 now has a winner ticket for the right game id.
        scenario.next_tx(user2);
        let ticket = scenario.take_from_address<GameWinner>(user2);
        assert!(ticket.get_game_winner_game_id() == &ticket_game_id, 1);
        test_scenario::return_to_address(user2, ticket);

        test_scenario::return_shared(game_val);
        scenario.end();
    }
}
