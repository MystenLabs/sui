// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module drand::scratch_card_tests {
    use drand::scratch_card::{Self, Game, Reward, Ticket, Winner};
    use sui::coin::{Self, Coin};
    use sui::sui::SUI;
    use sui::test_scenario::{Self as ts, Scenario};

    /// Taken from the output of
    /// curl https://drand.cloudflare.com/52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971/public/58810
    const ROUND_58810: vector<u8> =
        x"876b8586ed9522abd0ca596d6e214e9a7e9bedc4a2e9698d27970e892287268062aba93fd1a7c24fcc188a4c7f0a0e98";

    #[test]
    fun test_play_drand_scratch_card_with_winner() {
        let user1 = @0x0;
        let user2 = @0x1;

        let mut ts = ts::begin(user1);

        // Create the game and get back the output objects.
        let coin = ts.mint(10);
        scratch_card::create(coin, 10, 10, ts.ctx());

        ts.next_tx(user1);
        let game: Game = ts.take_immutable();
        assert!(game.end_drand_round() == 58810);

        let mut i = 0;
        loop {
            // User 2 buys a ticket.
            ts.next_tx(user2);
            let coin = ts.mint(1);
            let ticket = game.buy_ticket(coin, ts.ctx());
            transfer::public_transfer(ticket, user2);

            ts.next_tx(user1);
            let payment: Coin<SUI> = ts.take_from_sender();
            assert!(payment.value() == 1);
            ts.return_to_sender(payment);

            ts.next_tx(user2);
            let ticket: Ticket = ts.take_from_sender();
            ticket.evaluate(&game, ROUND_58810, ts.ctx());

            ts.next_tx(user2);
            if (ts.has_most_recent_for_sender<Winner>()) {
                break
            };
            i = i + 1;
        };

        // This process is deterministic, so we know that the 7th ticket will be a winner. This
        // value may change if ObjectIDs or transaction digests are changed.
        assert!(i == 7);

        // Claim the reward.
        ts.next_tx(user2);

        let winner: Winner = ts.take_from_sender();
        let mut reward: Reward = ts.take_shared();
        let winnings = winner.take_reward(&mut reward, ts.ctx());
        assert!(winnings.value() == 10);

        transfer::public_transfer(winnings, user2);
        ts::return_immutable(game);
        ts::return_shared(reward);
        ts.end();
    }

    #[test]
    fun test_play_drand_scratch_card_without_winner() {
        let user1 = @0x0;

        let mut ts = ts::begin(user1);

        // Create the game and get back the output objects.
        let coin = ts.mint(10);
        scratch_card::create(coin, 10, 10, ts.ctx());

        ts.next_tx(user1);

        // More 4 epochs forward.
        ts.next_epoch(user1);
        ts.next_epoch(user1);
        ts.next_epoch(user1);
        ts.next_epoch(user1);

        // Take back the reward.
        let game: Game = ts.take_immutable();
        let mut reward: Reward = ts.take_shared();
        reward.redeem(&game, ts.ctx());

        ts.next_tx(user1);

        let redeemed: Coin<SUI> = ts.take_from_sender();
        assert!(redeemed.value() == 10);

        ts.return_to_sender(redeemed);
        ts::return_shared(reward);
        ts::return_immutable(game);
        ts.end();
    }

    // === Test Helpers ===

    use fun mint as Scenario.mint;
    fun mint(ts: &mut Scenario, amount: u64): Coin<SUI> {
        coin::mint_for_testing(amount, ts.ctx())
    }
}
