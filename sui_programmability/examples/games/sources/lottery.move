// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A basic lottery game that depends on Sui randomness.
///
/// Anyone can create a new lottery game with an end time and a cost per ticket. After the end time, anyone can trigger
/// a function to determine the winner, and the owner of the winning ticket can redeem the entire balance of the game.
///
module games::lottery {
    use std::option::{Self, Option};
    use sui::balance::{Self, Balance};
    use sui::clock::{Self, Clock};
    use sui::coin::{Self, Coin};
    use sui::object::{Self, ID, UID};
    use sui::random::{Self, RandomnessRequest, RandomRounds};
    use sui::sui::SUI;
    use sui::transfer;
    use sui::tx_context::TxContext;

    /// Error codes
    const EGameInProgress: u64 = 0;
    const EGameAlreadyCompleted: u64 = 1;
    const ENoParticipants: u64 = 2;
    const EInvalidAmount: u64 = 3;
    const EGameMistmatch: u64 = 4;
    const ENotWinner: u64 = 4;
    const EWrongGameWinner: u64 = 5;

    /// Game represents a set of parameters of a single game.
    struct Game has key {
        id: UID,
        cost_in_sui: u64,
        participants: u32,
        end_time: u64,
        balance: Balance<SUI>,
        randomness_request: Option<RandomnessRequest>,
        winner: Option<u32>,
    }

    /// Ticket represents a participant in a single game.
    struct Ticket has key {
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
            balance: balance::zero(),
            randomness_request: option::none(),
            winner: option::none(),
        };
        transfer::share_object(game);
    }

    public fun set_randomness_request(game: &mut Game, rr: &RandomRounds, clock: &Clock, ctx: &mut TxContext) {
        assert!(game.end_time <= clock::timestamp_ms(clock), EGameInProgress);
        assert!(option::is_none(&game.randomness_request), EGameAlreadyCompleted);
        assert!(option::is_none(&game.winner), EGameAlreadyCompleted);
        game.randomness_request = option::some(random::create_randomness_request(rr, ctx));
    }

    public fun determine_winner(game: &mut Game, rr: &RandomRounds, ctx: &mut TxContext) {
        assert!(option::is_some(&game.randomness_request), 0); // TODO: error code
        assert!(option::is_none(&game.winner), EGameAlreadyCompleted);
        let randomness_request = option::extract(&mut game.randomness_request);
        let gen = random::fulfill_and_create_generator(&randomness_request, rr);
        let winner = random::generate_u32_in_range(&mut gen, 1, game.participants);
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

    /// The winner can take the prize.
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
    public fun get_winner_obj(game: &Game): Option<u32> {
        game.winner
    }

    #[test_only]
    public fun get_balance(game: &Game): u64 {
        balance::value(&game.balance)
    }

}
