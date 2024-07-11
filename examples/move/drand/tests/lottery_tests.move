// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module drand::lottery_tests {
    use sui::test_scenario::{Self as ts, Scenario};
    use drand::{
        lib::verify_time_has_passed,
        lottery::{Self, Game, Ticket, GameWinner},
    };

    /// Taken from the output of
    /// curl https://drand.cloudflare.com/52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971/public/8
    const ROUND_8: vector<u8> =
        x"a0c06b9964123d2e6036aa004c140fc301f4edd3ea6b8396a15dfd7dfd70cc0dce0b4a97245995767ab72cf59de58c47";

    /// Taken from theoutput of
    /// curl https://drand.cloudflare.com/52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971/public/10
    const ROUND_10: vector<u8> =
        x"ac415e508c484053efed1c6c330e3ae0bf20185b66ed088864dac1ff7d6f927610824986390d3239dac4dd73e6f865f5";

    #[test]
    fun test_verify_time_has_passed_success() {
        // exactly the 8th round
        verify_time_has_passed(1692803367 + 3*7, ROUND_8, 8);
        // the 8th round - 2 seconds
        verify_time_has_passed(1692803367 + 3*7 - 2, ROUND_8, 8);
    }

    #[test]
    #[expected_failure(abort_code = drand::lib::EInvalidProof)]
    fun test_verify_time_has_passed_failure() {
        // exactly the 9th round
        verify_time_has_passed(1692803367 + 3*8, ROUND_8, 8);
    }

    #[test]
    fun test_play_drand_lottery() {
        let user1 = @0x0;
        let user2 = @0x1;
        let user3 = @0x2;
        let user4 = @0x3;

        let mut ts = ts::begin(user1);

        lottery::create(10, ts.ctx());

        // Users each buy a ticket.
        ts.participate(user1);
        ts.participate(user2);
        ts.participate(user3);
        ts.participate(user4);

        // User 2 closes the game.
        ts.next_tx(user2);
        let mut game: Game = ts.take_shared();
        game.close(ROUND_8);
        ts::return_shared(game);

        // User 3 completes the game.
        ts.next_tx(user3);
        let mut game: Game = ts.take_shared();
        game.complete(ROUND_10);
        ts::return_shared(game);

        // User 2 is the winner since the mod of the hash results in 1.
        ts.next_tx(user2);
        assert!(!ts.has_most_recent_for_sender<GameWinner>());

        let game: Game = ts.take_shared();
        let ticket: Ticket = ts.take_from_sender();
        let winner = ticket.redeem(&game, ts.ctx());
        ticket.delete();

        // Make sure the ticket is for the right game.
        assert!(object::id(&game) == winner.game_id());

        transfer::public_transfer(winner, user2);
        ts::return_shared(game);
        ts.end();
    }

    // === Test Helpers ==

    use fun participate as Scenario.participate;
    fun participate(ts: &mut Scenario, user: address) {
        ts.next_tx(user);

        let mut game: Game = ts.take_shared();
        let ticket = game.participate(ts.ctx());

        transfer::public_transfer(ticket, user);
        ts::return_shared(game);
    }
}
