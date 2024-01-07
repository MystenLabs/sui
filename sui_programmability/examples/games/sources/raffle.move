// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A basic raffle game that depends on Sui randomness.
///
/// Anyone can create a new game with an end time and a cost per ticket. After the end time, anyone can trigger
/// the functions to determine the winner, and the owner of the winning ticket can redeem the entire balance of the game.
///

module games::raffle {
    use std::option::{Self, Option};
    use sui::balance::{Self, Balance};
    use sui::clock::{Self, Clock};
    use sui::coin::{Self, Coin};
    use sui::event;
    use sui::object::{Self, ID, UID};
    use sui::random::{Self, RandomGeneratorRequest, Random};
    use sui::sui::SUI;
    use sui::transfer;
    use sui::tx_context::TxContext;

    const EGameInProgress: u64 = 0;
    const EGameAlreadyClosed: u64 = 1;
    const ECloseNotCalled: u64 = 2;
    const EInvalidAmount: u64 = 3;
    const EGameMistmatch: u64 = 4;
    const EWrongGameWinner: u64 = 5;
    const EGameAlreadyCompleted: u64 = 6;

    /// Game represents a set of parameters of a single game.
    struct Game has key {
        id: UID,
        cost_in_sui: u64,
        participants: u32,
        end_time: u64,
        balance: Balance<SUI>,
        randomness_request: Option<RandomGeneratorRequest>,
        winner: Option<u32>,
    }

    /// Ticket represents a participant in a single game.
    struct Ticket has key {
        id: UID,
        game_id: ID,
        participant_index: u32,
    }

    /// Event for new randomness round.
    struct GameClosed has copy, drop {
        game_id: ID,
        waiting_for_round: u64,
    }

    /// Event for determined winner.
    struct WinnerDetermined has copy, drop {
        game_id: ID,
        winner: u32,
    }

    /// Create a shared-object Game.
    public fun create(end_time: u64, cost_in_sui: u64, ctx: &mut TxContext) {
        let game = Game {
            id: object::new(ctx),
            cost_in_sui,
            participants: 0,
            end_time,
            balance: balance::zero(),
            randomness_request: option::none(),
            winner: option::none(),
        };
        transfer::share_object(game);
    }

    /// Anyone can close the game after the end time. This fixes the randomness for determining the winner.
    public fun close_game(game: &mut Game, r: &Random, clock: &Clock, ctx: &mut TxContext) {
        assert!(game.end_time <= clock::timestamp_ms(clock), EGameInProgress);
        assert!(option::is_none(&game.randomness_request), EGameAlreadyClosed);
        assert!(option::is_none(&game.winner), EGameAlreadyCompleted);
        let req = random::new_request(r, ctx);
        // We use an event to notify the client side that the game was closed and a randomness request was created.
        // Alternatives: (1) Return the information in a new object to the user; (2) Inspect the game object directly.
        event::emit(GameClosed { game_id: object::id(game), waiting_for_round: random::required_round(&req) });
        game.randomness_request = option::some(req);
    }

    /// Anyone can determine the winner after the randomness has been fixed.
    public fun determine_winner(game: &mut Game, r: &Random) {
        assert!(option::is_some(&game.randomness_request), ECloseNotCalled);
        assert!(option::is_none(&game.winner), EGameAlreadyCompleted);
        let randomness_request = option::extract(&mut game.randomness_request);
        let gen = random::fulfill(&randomness_request, r);
        let winner = random::generate_u32_in_range(&mut gen, 1, game.participants);
        event::emit(WinnerDetermined { game_id: object::id(game), winner });
        game.winner = option::some(winner);
    }

    /// Anyone can play and receive a ticket.
    public fun play(game: &mut Game, coin: Coin<SUI>, clock: &Clock, ctx: &mut TxContext): Ticket {
        assert!(game.end_time > clock::timestamp_ms(clock), EGameAlreadyCompleted);
        assert!(coin::value(&coin) == game.cost_in_sui, EInvalidAmount);
        game.participants = game.participants + 1;
        coin::put(&mut game.balance, coin);
        Ticket {
            id: object::new(ctx),
            game_id: object::id(game),
            participant_index: game.participants,
        }
    }

    /// The winner can take the total balance.
    /// (Alternative design is to combine determine_winner and redeem into a single function.)
    public fun redeem(ticket: Ticket, game: &mut Game, ctx: &mut TxContext): Coin<SUI> {
        assert!(object::id(game) == ticket.game_id, EGameMistmatch);
        assert!(option::contains(&game.winner, &ticket.participant_index), EWrongGameWinner);
        destroy_ticket(ticket);
        let full_balance = balance::value(&game.balance);
        coin::take(&mut game.balance, full_balance, ctx)
    }

    public fun destroy_ticket(ticket: Ticket) {
        let Ticket { id, game_id:  _, participant_index: _} = ticket;
        object::delete(id);
    }

    #[test_only]
    public fun get_cost_in_sui(game: &Game): u64 {
        game.cost_in_sui
    }

    #[test_only]
    public fun get_end_time(game: &Game): u64 {
        game.end_time
    }

    #[test_only]
    public fun get_participants(game: &Game): u32 {
        game.participants
    }

    #[test_only]
    public fun get_winner(game: &Game): Option<u32> {
        game.winner
    }

    #[test_only]
    public fun get_balance(game: &Game): u64 {
        balance::value(&game.balance)
    }

    #[test_only]
    public fun get_randomness_request(game: &Game): &Option<RandomGeneratorRequest> {
        &game.randomness_request
    }

}
