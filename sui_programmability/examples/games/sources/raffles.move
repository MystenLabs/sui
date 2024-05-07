// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Basic raffles games that depends on Sui randomness.
///
/// Anyone can create a new raffle game with an end time and a price. After the end time, anyone can trigger
/// a function to determine the winner, and the winner gets the entire balance of the game.
///
/// - raffle_with_tickets uses tickets which could be transferred to other accounts, used as NFTs, etc.
/// - small_raffle uses a simpler approach with no tickets.

module games::raffle_with_tickets {
    use sui::balance::{Self, Balance};
    use sui::clock::{Self, Clock};
    use sui::coin::{Self, Coin};
    use sui::random::{Self, Random, new_generator};
    use sui::sui::SUI;

    /// Error codes
    const EGameInProgress: u64 = 0;
    const EGameAlreadyCompleted: u64 = 1;
    const EInvalidAmount: u64 = 2;
    const EGameMismatch: u64 = 3;
    const ENotWinner: u64 = 4;
    const ENoParticipants: u64 = 4;

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
        assert!(game.end_time <= clock::timestamp_ms(clock), EGameInProgress);
        assert!(option::is_none(&game.winner), EGameAlreadyCompleted);
        assert!(game.participants > 0, ENoParticipants);
        let mut generator = new_generator(r, ctx);
        let winner = random::generate_u32_in_range(&mut generator, 1, game.participants);
        game.winner = option::some(winner);
    }

    /// Anyone can play and receive a ticket.
    public fun buy_ticket(game: &mut Game, coin: Coin<SUI>, clock: &Clock, ctx: &mut TxContext): Ticket {
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
    public fun redeem(ticket: Ticket, game: Game, ctx: &mut TxContext): Coin<SUI> {
        assert!(object::id(&game) == ticket.game_id, EGameMismatch);
        assert!(option::contains(&game.winner, &ticket.participant_index), ENotWinner);
        destroy_ticket(ticket);

        let Game { id, cost_in_sui: _, participants: _, end_time: _, winner: _, balance } = game;
        object::delete(id);
        let reward = coin::from_balance(balance, ctx);
        reward
    }

    public fun destroy_ticket(ticket: Ticket) {
        let Ticket { id, game_id: _, participant_index: _ } = ticket;
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
}


module games::small_raffle {
    use sui::balance::{Self, Balance};
    use sui::clock::{Self, Clock};
    use sui::coin::{Self, Coin};
    use sui::random::{Self, Random, new_generator};
    use sui::sui::SUI;
    use sui::table::{Self, Table};
    use sui::tx_context::sender;

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
        participants_table: Table<u32, address>,
    }

    /// Create a shared-object Game.
    public fun create(end_time: u64, cost_in_sui: u64, ctx: &mut TxContext) {
        let game = Game {
            id: object::new(ctx),
            cost_in_sui,
            participants: 0,
            end_time,
            balance: balance::zero(),
            participants_table: table::new(ctx),
        };
        transfer::share_object(game);
    }

    /// Anyone can close the game and send the balance to the winner.
    ///
    /// The function is defined as private entry to prevent calls from other Move functions. (If calls from other
    /// functions are allowed, the calling function might abort the transaction depending on the winner.)
    /// Gas based attacks are not possible since the gas cost of this function is independent of the winner.
    entry fun close(game: Game, r: &Random, clock: &Clock, ctx: &mut TxContext) {
        assert!(game.end_time <= clock::timestamp_ms(clock), EGameInProgress);
        let Game { id, cost_in_sui: _, participants, end_time: _, balance, mut participants_table } = game;
        if (participants > 0) {
            let mut generator = new_generator(r, ctx);
            let winner = random::generate_u32_in_range(&mut generator, 1, participants);
            let winner_address = *table::borrow(&participants_table, winner);
            let reward = coin::from_balance(balance, ctx);
            transfer::public_transfer(reward, winner_address);
        } else {
            balance::destroy_zero(balance);
        };

        let mut i = 1;
        while (i <= participants) {
            table::remove(&mut participants_table, i);
            i = i + 1;
        };
        table::destroy_empty(participants_table);
        object::delete(id);
    }

    /// Anyone can play.
    public fun play(game: &mut Game, coin: Coin<SUI>, clock: &Clock, ctx: &mut TxContext) {
        assert!(game.end_time > clock::timestamp_ms(clock), EGameAlreadyCompleted);
        assert!(coin::value(&coin) == game.cost_in_sui, EInvalidAmount);
        assert!(game.participants < MaxParticipants, EReachedMaxParticipants);

        game.participants = game.participants + 1;
        coin::put(&mut game.balance, coin);
        table::add(&mut game.participants_table, game.participants, ctx.sender());
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
    public fun get_balance(game: &Game): u64 {
        balance::value(&game.balance)
    }
}
