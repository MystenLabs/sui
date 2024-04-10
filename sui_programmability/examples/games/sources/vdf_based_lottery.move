// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A basic lottery game that depends on user-provided randomness which is processed by a verifiable delay function (VDF)
/// to make sure that it is unbiasable. 
/// 
/// During the submission phase, players can buy tickets. When buying a ticket, a user must provide some randomness `r`. This 
/// randomness is added to the combined randomness of the lottery, `h`, as `h = Sha2_256(h, r)`.
/// 
/// After the submission phase has ended, the combined randomness is used to generate an input to the VDF. Anyone may now
/// compute the output and submit it along with a proof of correctness to the `complete` function. If the output and proof 
/// verifies, the game ends, and the hash of the output is used to pick a winner.
/// 
/// The outcome is guaranteed to be fair if:
///  1) At least one player contributes true randomness,
///  2) The number of iterations is defined such that it takes at least `submission_phase_length` to compute the VDF.
module games::vdf_based_lottery {
    use games::drand_lib::safe_selection;
    use std::option::{Self, Option};
    use sui::object::{Self, ID, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use sui::clock::{Self, Clock};
    use std::hash::sha2_256;

    /// Error codes
    const EGameNotInProgress: u64 = 0;
    const EGameAlreadyCompleted: u64 = 1;
    const EInvalidTicket: u64 = 3;
    const ESubmissionPhaseInProgress: u64 = 4;
    const EInvalidVdfOutput: u64 = 5;
    const ESubmissionPhaseFinished: u64 = 6;

    /// Game status
    const IN_PROGRESS: u8 = 0;
    const COMPLETED: u8 = 2;

    /// Use a fixed discriminant. In production we should use a larger one which is randomly generated.
    const DISCRIMINANT_BYTES: vector<u8> = x"fdf4aa9b7f49b85fc71f6fbf31a3d51e6828afb9d06165f5814bb5142485853abb52f50b7c8a937bba09ce75b51a639886d997d561b7a654f1a9e6b66645d76fad093381d464eccf28d599fb5a938bb99101c30e374f5f786c9232f56d0118826d113400b080bb4737018b088af5203a18da25d106fffdad7e8f660e141dd11f";

    /// Game represents a set of parameters of a single game.
    /// This game can be extended to require ticket purchase, reward winners, etc.
    ///
    struct Game has key, store {
        id: UID,
        iterations: u64,
        status: u8,
        timestamp_start: u64,
        submission_phase_length: u64,
        participants: u64,
        vdf_input_seed: vector<u8>,
        winner: Option<u64>,
    }

    /// Ticket represents a participant in a single game.
    /// Can be deconstructed only by the owner.
    struct Ticket has key, store {
        id: UID,
        game_id: ID,
        participant_index: u64,
    }

    /// GameWinner represents a participant that won in a specific game.
    /// Can be deconstructed only by the owner.
    struct GameWinner has key, store {
        id: UID,
        game_id: ID,
    }

    /// Create a shared-object Game.
    public fun create(iterations: u64, submission_phase_length: u64, clock: &Clock, ctx: &mut TxContext) {
        let game = Game {
            id: object::new(ctx),
            iterations: iterations,
            status: IN_PROGRESS,
            timestamp_start: clock.timestamp_ms(),
            submission_phase_length: submission_phase_length,
            vdf_input_seed: std::vector::empty<u8>(),
            participants: 0,
            winner: option::none(),
        };
        transfer::public_share_object(game);
    }

    /// Anyone can complete the game by providing the randomness of round.
    public fun complete(game: &mut Game, vdf_output: vector<u8>, vdf_proof: vector<u8>, clock: &Clock) {
        assert!(game.status != COMPLETED, EGameAlreadyCompleted);
        assert!(clock::timestamp_ms(clock) - game.timestamp_start >= game.submission_phase_length, ESubmissionPhaseInProgress);

        // Hash combined randomness to vdf input
        let discriminant = DISCRIMINANT_BYTES;
        let vdf_input = sui::vdf::hash_to_input(&discriminant, &game.vdf_input_seed);

        // Verify output and proof
        assert!(sui::vdf::vdf_verify(&discriminant, &vdf_input, &vdf_output, &vdf_proof, game.iterations), EInvalidVdfOutput);

        game.status = COMPLETED;

        // The randomness is derived from the VDF output by passing it through sha2_256 to make it uniform.
        let randomness = sha2_256(vdf_output);

        game.winner = option::some(safe_selection(game.participants, &randomness));
    }

    /// Anyone can participate in the game and receive a ticket.
    public fun participate(game: &mut Game, my_randomness: vector<u8>, clock: &Clock, ctx: &mut TxContext) {
        assert!(game.status == IN_PROGRESS, EGameNotInProgress);
        assert!(clock::timestamp_ms(clock) - game.timestamp_start < game.submission_phase_length, ESubmissionPhaseFinished);
        let ticket = Ticket {
            id: object::new(ctx),
            game_id: object::id(game),
            participant_index: game.participants,
        };

        // Update combined randomness
        let pack = std::vector::empty<u8>();
        pack.append(game.vdf_input_seed);
        pack.append(my_randomness);
        game.vdf_input_seed = sha2_256(pack);

        game.participants = game.participants + 1;
        transfer::public_transfer(ticket, ctx.sender());
    }

    /// The winner can redeem its ticket.
    public fun redeem(ticket: &Ticket, game: &Game, ctx: &mut TxContext) {
        assert!(object::id(game) == ticket.game_id, EInvalidTicket);
        assert!(game.winner.contains(&ticket.participant_index), EInvalidTicket);

        let winner = GameWinner {
            id: object::new(ctx),
            game_id: ticket.game_id,
        };
        transfer::public_transfer(winner, tx_context::sender(ctx));
    }

    // Note that a ticket can be deleted before the game was completed.
    public fun delete_ticket(ticket: Ticket) {
        let Ticket { id, game_id:  _, participant_index: _} = ticket;
        object::delete(id);
    }

    public fun delete_game_winner(ticket: GameWinner) {
        let GameWinner { id, game_id:  _} = ticket;
        object::delete(id);
    }
    public use fun ticket_game_id as Ticket.game_id;
    public fun ticket_game_id(ticket: &Ticket): &ID {
        &ticket.game_id
    }

    public use fun game_winner_game_id as GameWinner.game_id;
    public fun game_winner_game_id(ticket: &GameWinner): &ID {
        &ticket.game_id
    }

}
