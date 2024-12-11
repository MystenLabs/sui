// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Basic raffles game that depends on Sui randomness.
///
/// Anyone can create a new raffle game with an end time and a price. After the end time, anyone can trigger
/// a function to determine the winner, and the winner gets the entire balance of the game.
///
/// Example 2: Small raffle, without tickets.

module raffles::example2;

use sui::{
    balance::{Self, Balance},
    clock::Clock,
    coin::{Self, Coin},
    random::{Random, new_generator},
    sui::SUI,
    table_vec::{Self, TableVec},
    tx_context::sender
};

/// Error codes
const EGameInProgress: u64 = 0;
const EGameAlreadyCompleted: u64 = 1;
const EInvalidAmount: u64 = 2;
const EReachedMaxParticipants: u64 = 3;

const MaxParticipants: u32 = 500;

/// Game represents a set of parameters of a single game.
public struct Game has key {
    id: UID,
    cost_in_sui: u64,
    participants: u32,
    end_time: u64,
    balance: Balance<SUI>,
    participants_table: TableVec<address>,
}

/// Create a shared-object Game.
public fun create(end_time: u64, cost_in_sui: u64, ctx: &mut TxContext) {
    let game = Game {
        id: object::new(ctx),
        cost_in_sui,
        participants: 0,
        end_time,
        balance: balance::zero(),
        participants_table: table_vec::empty(ctx),
    };
    transfer::share_object(game);
}

/// Anyone can close the game and send the balance to the winner.
///
/// The function is defined as private entry to prevent calls from other Move functions. (If calls from other
/// functions are allowed, the calling function might abort the transaction depending on the winner.)
/// Gas based attacks are not possible since the gas cost of this function is independent of the winner.
entry fun close(game: Game, r: &Random, clock: &Clock, ctx: &mut TxContext) {
    assert!(game.end_time <= clock.timestamp_ms(), EGameInProgress);
    let Game { id, cost_in_sui: _, participants, end_time: _, balance, participants_table } = game;
    if (participants > 0) {
        let mut generator = r.new_generator(ctx);
        let winner = generator.generate_u32_in_range(0, participants - 1);
        let winner_address = participants_table[winner as u64];
        let reward = coin::from_balance(balance, ctx);
        transfer::public_transfer(reward, winner_address);
    } else {
        balance.destroy_zero();
    };

    participants_table.drop();
    object::delete(id);
}

/// Anyone can play.
public fun play(game: &mut Game, coin: Coin<SUI>, clock: &Clock, ctx: &mut TxContext) {
    assert!(game.end_time > clock.timestamp_ms(), EGameAlreadyCompleted);
    assert!(coin.value() == game.cost_in_sui, EInvalidAmount);
    assert!(game.participants < MaxParticipants, EReachedMaxParticipants);

    coin::put(&mut game.balance, coin);
    game.participants_table.push_back(ctx.sender());
    game.participants = game.participants + 1;
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
public fun balance(game: &Game): u64 {
    game.balance.value()
}
