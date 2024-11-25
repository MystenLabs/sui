// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Basic raffle game that depends on Sui randomness.
///
/// Anyone can create a new raffle game with an end time and a price. After the end time, anyone can trigger
/// a function to determine the winner, and the winner gets the entire balance of the game.
///
/// Example 1: Uses tickets which could be transferred to other accounts, used as NFTs, etc.

module raffles::example1;

use sui::{
    balance::{Self, Balance},
    clock::Clock,
    coin::{Self, Coin},
    random::{Random, new_generator},
    sui::SUI
};

/// Error codes
const EGameInProgress: u64 = 0;
const EGameAlreadyCompleted: u64 = 1;
const EInvalidAmount: u64 = 2;
const EGameMismatch: u64 = 3;
const ENotWinner: u64 = 4;
const ENoParticipants: u64 = 5;

/// Game represents a set of parameters of a single game.
public struct Game has key {
    id: UID,
    cost_in_sui: u64,
    participants: u32,
    end_time: u64,
    winner: Option<u32>,
    balance: Balance<SUI>,
}

/// Ticket represents a participant in a single game.
public struct Ticket has key {
    id: UID,
    game_id: ID,
    participant_index: u32,
}

/// Create a shared-object Game.
public fun create(end_time: u64, cost_in_sui: u64, ctx: &mut TxContext) {
    let game = Game {
        id: object::new(ctx),
        cost_in_sui,
        participants: 0,
        end_time,
        winner: option::none(),
        balance: balance::zero(),
    };
    transfer::share_object(game);
}

/// Anyone can determine a winner.
///
/// The function is defined as private entry to prevent calls from other Move functions. (If calls from other
/// functions are allowed, the calling function might abort the transaction depending on the winner.)
/// Gas based attacks are not possible since the gas cost of this function is independent of the winner.
entry fun determine_winner(game: &mut Game, r: &Random, clock: &Clock, ctx: &mut TxContext) {
    assert!(game.end_time <= clock.timestamp_ms(), EGameInProgress);
    assert!(game.winner.is_none(), EGameAlreadyCompleted);
    assert!(game.participants > 0, ENoParticipants);
    let mut generator = r.new_generator(ctx);
    let winner = generator.generate_u32_in_range(1, game.participants);
    game.winner = option::some(winner);
}

/// Anyone can play and receive a ticket.
public fun buy_ticket(
    game: &mut Game,
    coin: Coin<SUI>,
    clock: &Clock,
    ctx: &mut TxContext,
): Ticket {
    assert!(game.end_time > clock.timestamp_ms(), EGameAlreadyCompleted);
    assert!(coin.value() == game.cost_in_sui, EInvalidAmount);

    game.participants = game.participants + 1;
    coin::put(&mut game.balance, coin);

    Ticket {
        id: object::new(ctx),
        game_id: object::id(game),
        participant_index: game.participants,
    }
}

/// The winner can take the prize.
public fun redeem(ticket: Ticket, game: Game, ctx: &mut TxContext): Coin<SUI> {
    assert!(object::id(&game) == ticket.game_id, EGameMismatch);
    assert!(game.winner.contains(&ticket.participant_index), ENotWinner);
    destroy_ticket(ticket);

    let Game { id, cost_in_sui: _, participants: _, end_time: _, winner: _, balance } = game;
    object::delete(id);
    let reward = balance.into_coin(ctx);
    reward
}

public use fun destroy_ticket as Ticket.destroy;

public fun destroy_ticket(ticket: Ticket) {
    let Ticket { id, game_id: _, participant_index: _ } = ticket;
    object::delete(id);
}

#[test_only]
public fun cost_in_sui(game: &Game): u64 {
    game.cost_in_sui
}

#[test_only]
public fun end_time(game: &Game): u64 {
    game.end_time
}

#[test_only]
public fun participants(game: &Game): u32 {
    game.participants
}

#[test_only]
public fun winner(game: &Game): Option<u32> {
    game.winner
}

#[test_only]
public fun balance(game: &Game): u64 {
    game.balance.value()
}
