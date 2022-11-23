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
        // curl https://drand.cloudflare.com/8990e7a9aaed2ffed73dbd7092123d6f289930540d7651336225dc172e51b2ce/public/8
        verify_time_has_passed(
            1595431050 + 30*7, // exactly the 8th round
            x"b3ed3c540ef5c5407ea6dbf7407ca5899feeb54f66f7e700ee063db71f979a869d28efa9e10b5e6d3d24a838e8b6386a15b411946c12815d81f2c445ae4ee1a7732509f0842f327c4d20d82a1209f12dbdd56fd715cc4ed887b53c321b318cd7",
            x"ada04f01558359fec41abeee43c5762c4017476a1e64ad643d3378a50ac1f7d07ad0abf0ba4bada53e6762582d661a980adf6290b5fb1683dedd821fe192868d70624907b2cef002e3ee197acd2395f1406fb660c91337d505860ab306a4432e",
            8
        );
        verify_time_has_passed(
            1595431050 + 30*7 - 10, // the 8th round - 10 seconds
            x"b3ed3c540ef5c5407ea6dbf7407ca5899feeb54f66f7e700ee063db71f979a869d28efa9e10b5e6d3d24a838e8b6386a15b411946c12815d81f2c445ae4ee1a7732509f0842f327c4d20d82a1209f12dbdd56fd715cc4ed887b53c321b318cd7",
            x"ada04f01558359fec41abeee43c5762c4017476a1e64ad643d3378a50ac1f7d07ad0abf0ba4bada53e6762582d661a980adf6290b5fb1683dedd821fe192868d70624907b2cef002e3ee197acd2395f1406fb660c91337d505860ab306a4432e",
            8
        );
    }

    #[test]
    #[expected_failure(abort_code = 1)]
    fun test_verify_time_has_passed_failure() {
        // Taken from the output of
        // curl https://drand.cloudflare.com/8990e7a9aaed2ffed73dbd7092123d6f289930540d7651336225dc172e51b2ce/public/8
        verify_time_has_passed(
            1595431050 + 30*8, // exactly the 9th round - 10 seconds
            x"b3ed3c540ef5c5407ea6dbf7407ca5899feeb54f66f7e700ee063db71f979a869d28efa9e10b5e6d3d24a838e8b6386a15b411946c12815d81f2c445ae4ee1a7732509f0842f327c4d20d82a1209f12dbdd56fd715cc4ed887b53c321b318cd7",
            x"ada04f01558359fec41abeee43c5762c4017476a1e64ad643d3378a50ac1f7d07ad0abf0ba4bada53e6762582d661a980adf6290b5fb1683dedd821fe192868d70624907b2cef002e3ee197acd2395f1406fb660c91337d505860ab306a4432e",
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
        // curl https://drand.cloudflare.com/8990e7a9aaed2ffed73dbd7092123d6f289930540d7651336225dc172e51b2ce/public/8
        drand_based_lottery::close(
            game,
            x"b3ed3c540ef5c5407ea6dbf7407ca5899feeb54f66f7e700ee063db71f979a869d28efa9e10b5e6d3d24a838e8b6386a15b411946c12815d81f2c445ae4ee1a7732509f0842f327c4d20d82a1209f12dbdd56fd715cc4ed887b53c321b318cd7",
            x"ada04f01558359fec41abeee43c5762c4017476a1e64ad643d3378a50ac1f7d07ad0abf0ba4bada53e6762582d661a980adf6290b5fb1683dedd821fe192868d70624907b2cef002e3ee197acd2395f1406fb660c91337d505860ab306a4432e"
        );

        // User3 completes the game.
        test_scenario::next_tx(scenario, user3);
        // Taken from theoutput of
        // curl https://drand.cloudflare.com/8990e7a9aaed2ffed73dbd7092123d6f289930540d7651336225dc172e51b2ce/public/8
        drand_based_lottery::complete(
            game,
            x"aec34e398bb53efc192ef6b91ad6960689aefa2c8326c521523d922849bb8bc16e76872640e7a1dd656e94772d9fd4ae19a63a10854a0853505bd3c8c5b8fff109ff260b0566b5ac93d2b0d8fecc9b08f7ad5101a253913f55a0c53f45c15c7f",
            x"99c37c83a0d7bb637f0e2f0c529aa5c8a37d0287535debe5dacd24e95b6e38f3394f7cb094bdf4908a192a3563276f951948f013414d927e0ba8c84466b4c9aea4de2a253dfec6eb5b323365dfd2d1cb98184f64c22c5293c8bfe7962d4eb0f5"
        );

        // User3 is the winner since the mod of the hash results in 2.
        test_scenario::next_tx(scenario, user3);
        assert!(!test_scenario::has_most_recent_for_address<GameWinner>(user3), 1);
        let ticket = test_scenario::take_from_address<Ticket>(scenario, user3);
        let ticket_game_id = *drand_based_lottery::get_ticket_game_id(&ticket);
        drand_based_lottery::redeem(&ticket, &game_val, test_scenario::ctx(scenario));
        drand_based_lottery::delete_ticket(ticket);

        // Make sure User3 now has a winner ticket for the right game id.
        test_scenario::next_tx(scenario, user3);
        let ticket = test_scenario::take_from_address<GameWinner>(scenario, user3);
        assert!(drand_based_lottery::get_game_winner_game_id(&ticket) == &ticket_game_id, 1);
        test_scenario::return_to_address(user3, ticket);

        test_scenario::return_shared(game_val);
        test_scenario::end(scenario_val);
    }
}
