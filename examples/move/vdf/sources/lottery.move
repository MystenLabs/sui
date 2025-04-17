// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A basic lottery game that depends on user-provided randomness which is processed by a verifiable
/// delay function (VDF) to make sure that it is unbiasable.
///
/// During the submission phase, players can buy tickets. When buying a ticket, a user must provide
/// some randomness `r`. This randomness is added to the combined randomness of the lottery, `h`, as
/// `h = Sha2_256(h, r)`.
///
/// After the submission phase has ended, the combined randomness is used to generate an input to
/// the VDF. Anyone may now compute the output and submit it along with a proof of correctness to
/// the `complete` function. If the output and proof verifies, the game ends, and the hash of the
/// output is used to pick a winner.
///
/// The outcome is guaranteed to be fair if:
///
///  1) At least one player contributes true randomness,
///
///  2) The number of iterations is defined such that it takes at least `submission_phase_length` to
///     compute the VDF.
module vdf::lottery;

use std::hash::sha2_256;
use sui::{clock::Clock, vdf::{hash_to_input, vdf_verify}};

// === Receiver Functions ===

public use fun delete_ticket as Ticket.delete;
public use fun delete_game_winner as GameWinner.delete;
public use fun ticket_game_id as Ticket.game_id;
public use fun game_winner_game_id as GameWinner.game_id;

// === Object Types ===

/// Game represents a set of parameters of a single game.
/// This game can be extended to require ticket purchase, reward winners, etc.
public struct Game has key {
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
public struct Ticket has key, store {
    id: UID,
    game_id: ID,
    participant_index: u64,
}

/// GameWinner represents a participant that won in a specific game.
/// Can be deconstructed only by the owner.
public struct GameWinner has key, store {
    id: UID,
    game_id: ID,
}

// === Error Codes ===

#[error]
const EGameNotInProgress: vector<u8> = b"Lottery not in progress, cannot participate.";

#[error]
const EGameAlreadyCompleted: vector<u8> = b"Lottery winner has already been selected";

#[error]
const EInvalidTicket: vector<u8> = b"Ticket does not match lottery";

#[error]
const ENotWinner: vector<u8> = b"Not the winning ticket";

#[error]
const ESubmissionPhaseInProgress: vector<u8> =
    b"Cannot call winner or redeem funds until submission phase has completed.";

#[error]
const EInvalidVdfProof: vector<u8> = b"Invalid VDF Proof";

#[error]
const ESubmissionPhaseFinished: vector<u8> = b"Cannot participate in a finished lottery.";

#[error]
const EInvalidRandomness: vector<u8> = b"Randomness length is not correct";

// === Constants ===

// Game status
const IN_PROGRESS: u8 = 0;
const COMPLETED: u8 = 1;

const RANDOMNESS_LENGTH: u64 = 16;

// === Public Functions ===

/// Create a shared-object Game.
public fun create(
    iterations: u64,
    submission_phase_length: u64,
    clock: &Clock,
    ctx: &mut TxContext,
) {
    transfer::share_object(Game {
        id: object::new(ctx),
        iterations,
        status: IN_PROGRESS,
        timestamp_start: clock.timestamp_ms(),
        submission_phase_length,
        vdf_input_seed: vector::empty<u8>(),
        participants: 0,
        winner: option::none(),
    });
}

/// Anyone can participate in the game and receive a ticket.
public fun participate(
    self: &mut Game,
    my_randomness: vector<u8>,
    clock: &Clock,
    ctx: &mut TxContext,
): Ticket {
    assert!(self.status == IN_PROGRESS, EGameNotInProgress);
    assert!(
        clock.timestamp_ms() - self.timestamp_start < self.submission_phase_length,
        ESubmissionPhaseFinished,
    );

    // Update combined randomness by concatenating the provided randomness and hashing it
    assert!(my_randomness.length() == RANDOMNESS_LENGTH, EInvalidRandomness);
    self.vdf_input_seed.append(my_randomness);
    self.vdf_input_seed = sha2_256(self.vdf_input_seed);

    // Assign index to this participant
    let participant_index = self.participants;
    self.participants = self.participants + 1;

    Ticket {
        id: object::new(ctx),
        game_id: object::id(self),
        participant_index,
    }
}

/// Complete this lottery by sending VDF output and proof for the seed created from the
/// contributed randomness. Anyone can call this.
public fun complete(self: &mut Game, vdf_output: vector<u8>, vdf_proof: vector<u8>, clock: &Clock) {
    assert!(self.status != COMPLETED, EGameAlreadyCompleted);
    assert!(
        clock.timestamp_ms() - self.timestamp_start >= self.submission_phase_length,
        ESubmissionPhaseInProgress,
    );

    // Hash combined randomness to vdf input
    let vdf_input = hash_to_input(&self.vdf_input_seed);

    // Verify output and proof
    assert!(vdf_verify(&vdf_input, &vdf_output, &vdf_proof, self.iterations), EInvalidVdfProof);

    // The randomness is derived from the VDF output by passing it through a hash function with
    // uniformly distributed output to make it uniform. Any hash function with uniformly
    // distributed output can be used.
    let randomness = sha2_256(vdf_output);

    // Set winner and mark lottery completed
    self.winner = option::some(safe_selection(self.participants, &randomness));
    self.status = COMPLETED;
}

/// The winner can redeem its ticket.
public fun redeem(self: &Game, ticket: &Ticket, ctx: &mut TxContext): GameWinner {
    assert!(self.status == COMPLETED, ESubmissionPhaseInProgress);
    assert!(object::id(self) == ticket.game_id, EInvalidTicket);
    assert!(self.winner.contains(&ticket.participant_index), ENotWinner);

    GameWinner {
        id: object::new(ctx),
        game_id: ticket.game_id,
    }
}

// Note that a ticket can be deleted before the game was completed.
public fun delete_ticket(ticket: Ticket) {
    let Ticket { id, game_id: _, participant_index: _ } = ticket;
    object::delete(id);
}

public fun delete_game_winner(ticket: GameWinner) {
    let GameWinner { id, game_id: _ } = ticket;
    object::delete(id);
}

public fun ticket_game_id(ticket: &Ticket): &ID {
    &ticket.game_id
}

public fun game_winner_game_id(ticket: &GameWinner): &ID {
    &ticket.game_id
}

// === Private Helpers ===

// Converts the first 16 bytes of rnd to a u128 number and outputs its modulo with input n.
// Since n is u64, the output is at most 2^{-64} biased assuming rnd is uniformly random.
fun safe_selection(n: u64, rnd: &vector<u8>): u64 {
    assert!(rnd.length() >= 16, EInvalidRandomness);
    let mut m: u128 = 0;
    let mut i = 0;
    while (i < 16) {
        m = m << 8;
        let curr_byte = rnd[i];
        m = m + (curr_byte as u128);
        i = i + 1;
    };
    let n_128 = (n as u128);
    let module_128 = m % n_128;
    let res = (module_128 as u64);
    res
}
