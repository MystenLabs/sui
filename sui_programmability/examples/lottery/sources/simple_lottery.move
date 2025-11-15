// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Simple lottery module for on-chain random draws.
///
/// Players buy tickets with SUI, and a winner is selected
/// when the lottery ends. The winner receives the entire prize pool.
module lottery::simple_lottery {
    use sui::object::{Self, Info, UID};
    use sui::coin::{Self, Coin};
    use sui::sui::SUI;
    use sui::balance::{Self, Balance};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use std::vector;

    /// Error codes
    const ELotteryEnded: u64 = 0;
    const ELotteryNotEnded: u64 = 1;
    const EInsufficientPayment: u64 = 2;
    const ENoParticipants: u64 = 3;
    const ENotAuthorized: u64 = 4;
    const EAlreadyDrawn: u64 = 5;

    /// A lottery game
    struct Lottery has key {
        id: UID,
        organizer: address,
        ticket_price: u64,
        prize_pool: Balance<SUI>,
        participants: vector<address>,
        end_time: u64,
        winner: Option<address>,
        drawn: bool,
    }

    /// Create a new lottery
    public entry fun create_lottery(
        ticket_price: u64,
        end_time: u64,
        ctx: &mut TxContext
    ) {
        let lottery = Lottery {
            id: object::new(ctx),
            organizer: tx_context::sender(ctx),
            ticket_price,
            prize_pool: balance::zero(),
            participants: vector::empty(),
            end_time,
            winner: option::none(),
            drawn: false,
        };

        transfer::share_object(lottery);
    }

    /// Buy a lottery ticket
    public entry fun buy_ticket(
        lottery: &mut Lottery,
        payment: Coin<SUI>,
        ctx: &mut TxContext
    ) {
        // Check lottery is still active
        assert!(tx_context::epoch(ctx) < lottery.end_time, ELotteryEnded);

        // Check payment is sufficient
        let amount = coin::value(&payment);
        assert!(amount >= lottery.ticket_price, EInsufficientPayment);

        // Add payment to prize pool
        let payment_balance = coin::into_balance(payment);
        balance::join(&mut lottery.prize_pool, payment_balance);

        // Add participant
        let buyer = tx_context::sender(ctx);
        vector::push_back(&mut lottery.participants, buyer);
    }

    /// Draw the winner (simplified random selection)
    public entry fun draw_winner(
        lottery: &mut Lottery,
        ctx: &mut TxContext
    ) {
        // Check lottery has ended
        assert!(tx_context::epoch(ctx) >= lottery.end_time, ELotteryNotEnded);

        // Check not already drawn
        assert!(!lottery.drawn, EAlreadyDrawn);

        // Check there are participants
        assert!(vector::length(&lottery.participants) > 0, ENoParticipants);

        // Simple "random" selection based on epoch
        // In production, use a proper randomness source
        let random_index = tx_context::epoch(ctx) % vector::length(&lottery.participants);
        let winner_address = *vector::borrow(&lottery.participants, random_index);

        lottery.winner = option::some(winner_address);
        lottery.drawn = true;
    }

    /// Claim the prize (by winner)
    public entry fun claim_prize(
        lottery: &mut Lottery,
        ctx: &mut TxContext
    ) {
        // Check lottery has been drawn
        assert!(lottery.drawn, ELotteryNotEnded);

        // Check sender is the winner
        let sender = tx_context::sender(ctx);
        assert!(option::contains(&lottery.winner, &sender), ENotAuthorized);

        // Transfer prize pool to winner
        let prize_amount = balance::value(&lottery.prize_pool);
        let prize = coin::take(&mut lottery.prize_pool, prize_amount, ctx);
        transfer::transfer(prize, sender);
    }

    /// Cancel lottery and refund participants (only by organizer, before draw)
    public entry fun cancel_lottery(
        lottery: &mut Lottery,
        ctx: &mut TxContext
    ) {
        // Check sender is organizer
        assert!(tx_context::sender(ctx) == lottery.organizer, ENotAuthorized);

        // Check not already drawn
        assert!(!lottery.drawn, EAlreadyDrawn);

        // In a real implementation, would refund all participants
        // For simplicity, we mark it as ended
        lottery.drawn = true;
    }

    /// View functions

    /// Get lottery info
    public fun get_info(lottery: &Lottery): (u64, u64, u64, bool) {
        (
            lottery.ticket_price,
            balance::value(&lottery.prize_pool),
            lottery.end_time,
            lottery.drawn
        )
    }

    /// Get number of participants
    public fun participant_count(lottery: &Lottery): u64 {
        vector::length(&lottery.participants)
    }

    /// Get winner if drawn
    public fun get_winner(lottery: &Lottery): Option<address> {
        lottery.winner
    }

    /// Check if address participated
    public fun has_participated(lottery: &Lottery, addr: address): bool {
        vector::contains(&lottery.participants, &addr)
    }

    /// Get prize pool amount
    public fun prize_pool_amount(lottery: &Lottery): u64 {
        balance::value(&lottery.prize_pool)
    }
}
