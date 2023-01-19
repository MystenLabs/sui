// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Example of using Randomness for a lottery based on shared objects.
/// - Anyone can participate in the lottery by purchasing a ticket.
/// - Anyone can trigger the lottery to be drawn.
/// - Only the winner can claim the prize.
module games::randomness_based_lottery {
    use std::option;
    use sui::balance::{Self, Balance, zero};
    use sui::coin::{Self, Coin};
    use sui::object::{Self, UID, ID};
    use sui::randomness;
    use sui::sui::SUI;
    use sui::tx_context::{Self, TxContext};

    const EWrongPrice: u64 = 1;
    const ELotteryClosed: u64 = 2;
    const ETooManyTickets: u64 = 3;
    const EWrongRandomness: u64 = 4;
    const EWinnerAlreadyDetermined: u64 = 5;
    const EWinnerNotDetermined: u64 = 6;
    const EWrongTicket: u64 = 7;
    const EWrongLottery: u64 = 8;

    /// Shared object that collects the coins from the players and distributes them to the winner.
    struct Lottery has key {
        id: UID,
        balance: Balance<SUI>,
        participants: u8,
        randomness_id: option::Option<ID>,
        winner: option::Option<u8>,
    }

    struct Ticket has key {
        id: UID,
        lottery_id: ID,
        participant_id: u8,
    }

    struct RANDOMNESS_WITNESS has drop {}

    /// Create a new lottery.
    public entry fun create(ctx: &mut TxContext) {
        let lottery = Lottery {
            id: object::new(ctx),
            balance: zero(),
            participants: 0,
            randomness_id: option::none(),
            winner: option::none(),
        };
        sui::transfer::share_object(lottery);
    }

    /// Buy a ticket for the lottery.
    public entry fun buy_ticket(lottery: &mut Lottery, coin: Coin<SUI>, ctx: &mut TxContext) {
        assert!(coin::value(&coin) == 1, EWrongPrice);
        assert!(option::is_none(&lottery.randomness_id), ELotteryClosed);
        assert!(lottery.participants < 250, ETooManyTickets);
        balance::join(&mut lottery.balance, coin::into_balance(coin));
        let ticket = Ticket {
            id: object::new(ctx),
            lottery_id: object::id(lottery),
            participant_id: lottery.participants,
        };
        lottery.participants = lottery.participants + 1;
        sui::transfer::transfer(ticket, tx_context::sender(ctx));
    }

    /// Stop selling tickets and create the Randomness that will determine the winner.
    public entry fun close(lottery: &mut Lottery, ctx: &mut TxContext) {
        assert!(option::is_none(&lottery.randomness_id), ELotteryClosed);
        let r = randomness::new(RANDOMNESS_WITNESS {}, ctx);
        let randomness_id = object::id(&r);
        lottery.randomness_id = option::some(randomness_id);
        randomness::share_object(r);
    }

    /// Draw the winner.
    public entry fun determine_winner(
        lottery: &mut Lottery,
        randomness: &mut randomness::Randomness<RANDOMNESS_WITNESS>,
        sig: vector<u8>
    ) {
        assert!(lottery.randomness_id == option::some(object::id(randomness)), EWrongRandomness);
        assert!(option::is_none(&lottery.winner), EWinnerAlreadyDetermined);
        randomness::set(randomness, sig);
        let random_bytes = option::borrow(randomness::value(randomness));
        let winner = randomness::safe_selection((lottery.participants as u64), random_bytes);
        std::debug::print(&winner);
        lottery.winner = option::some((winner as u8));
    }

    /// Claim the prize.
    public entry fun claim_prize(lottery: &mut Lottery, ticket: Ticket, ctx: &mut TxContext) {
        assert!(option::is_some(&lottery.winner), EWinnerNotDetermined);
        assert!(ticket.lottery_id == object::id(lottery), EWrongLottery);
        assert!(ticket.participant_id == *option::borrow(&lottery.winner), EWrongTicket);
        let amount = balance::value(&lottery.balance);
        let coin = coin::from_balance(balance::split(&mut lottery.balance, amount), ctx);
        sui::pay::keep(coin, ctx);
        let Ticket { id, lottery_id: _, participant_id: _} = ticket;
        object::delete(id);
    }
}