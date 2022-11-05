// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A basic game that depends on randomness from drand.
//
// The main chain of drand creates random 32 bytes every 30 seconds. This randomness is verifiable in the sense
// that anyone can check if a given 32 bytes bytes are indeed the i-th output of drand. For more details see
// https://drand.love/
//
// One could implement on-chain games that need unbiasable and unpredictable randomness using drand as the source of
// randomness. I.e., every time the game needs randomness, it receives the next 32 bytes from drand (whether as part
// of a transaction or by reading it from an existing object) and follows accordingly.
// However, this simplistic flow is insecure in some cases as the blockchain is not aware of the latest round of drand,
// and thus it may depend on randomness that is already public to everyone.
//
// Below we design a game that overcomes this issue as following:
// - A game is defined for a specific drand round N in the future. N can be, for example, the round that is expected in
//   5 mins from now, where the current round can be retrieved (off-chain) using
//   `curl https://drand.cloudflare.com/8990e7a9aaed2ffed73dbd7092123d6f289930540d7651336225dc172e51b2ce/public/latest'.
// - Anyone can "close" the game to new participants by providing drand's randomness of round N-2 (i.e., 1 minute before
//   round N).
// - Users can join the game as long as it is not closed and receive a "ticket".
// - Anyone can "complete" the game by proving drand's randomness of round N, which is used to declare the winner.
// - The owner of the winning "ticket" can request a "winner ticket" and finish the game.
// As long as someone is closing the game in time (or at least before round N) we have the guarantee that the winner is
// selected using unpredictable and unbiasable randomness. Otherwise, someone could wait until the randomness of round N
// is public, see if it could won the game and if so, join the game and drive it to completion.
//
// All the external inputs needed for the following APIs can be retrieved from one of drand's public APIs, e.g. using
// the above curl command.
//
// TODO code for round number from epoch
module games::drand_random_selection {
    use std::vector;

    use sui::object::{Self, ID, UID};
    use std::option::{Self, Option};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use sui::bls12381;
    use std::hash::sha2_256;

    // Error codes
    const EGameNotInProgress: u64 = 0;
    const EGameNotClosed: u64 = 1;
    const EInvalidRandomness: u64 = 2;
    const EInvalidSeed: u64 = 3;
    const EInvalidTicket: u64 = 4;

    // Game status
    const IN_PROGRESS: u8 = 0;
    const CLOSED: u8 = 1;
    const COMPLETED: u8 = 2;

    // Can be deconstructed only by the winner.
    struct Game has key, store {
        id: UID,
        round: u64,
        status: u8,
        participants: u64,
        winner: Option<u64>,
    }

    // Can be deconstructed by the owner.
    struct Ticket has key, store {
        id: UID,
        game_id: ID,
        participant_index: u64,
    }

    // Can be deconstructed by the owner.
    struct WinnerTicket has key, store {
        id: UID,
        game_id: ID,
    }

    // Create a shared object Game.
    public entry fun create(round: u64, ctx: &mut TxContext) {
        let game = Game {
            id: object::new(ctx),
            round,
            status: IN_PROGRESS,
            participants: 0,
            winner: option::none(),
        };
        transfer::share_object(game);
    }

    public entry fun close(game: &mut Game, drand_sig: vector<u8>, drand_prev_sig: vector<u8>) {
        assert!(game.status == IN_PROGRESS, EGameNotInProgress);
        assert!(
            verify_drand_signature(DRAND_PUBLIC_KEY, drand_sig, drand_prev_sig, closing_round(game.round)) == true,
            EInvalidRandomness
        );
        game.status = CLOSED;
    }

    public entry fun complete(game: &mut Game, drand_sig: vector<u8>, drand_prev_sig: vector<u8>) {
        assert!(game.status == CLOSED, EGameNotClosed);
        assert!(
            verify_drand_signature(DRAND_PUBLIC_KEY, drand_sig, drand_prev_sig, game.round) == true,
            EInvalidRandomness
        );
        game.status = COMPLETED;
        let digest = sha2_256(drand_sig);
        game.winner = option::some(safe_selection(game.participants, digest));
    }

    public entry fun participate(game: &mut Game, ctx: &mut TxContext) {
        assert!(game.status == IN_PROGRESS, EGameNotInProgress);
        let ticket = Ticket {
            id: object::new(ctx),
            game_id: object::id(game),
            participant_index: game.participants,
        };
        game.participants = game.participants + 1;
        transfer::transfer(ticket, tx_context::sender(ctx));
    }

    public entry fun collect_my_winner_ticket(ticket: &Ticket, game: Game, ctx: &mut TxContext) {
        assert!(object::id(&game) == ticket.game_id, EInvalidTicket);
        assert!(option::contains(&game.winner, &ticket.participant_index), EInvalidTicket);

        let Game { id, status: _, round: _, participants: _, winner: _ } = game;
        object::delete(id);

        let winner = WinnerTicket {
            id: object::new(ctx),
            game_id: ticket.game_id,
        };
        transfer::transfer(winner, tx_context::sender(ctx));
    }

    // Note that a ticket can be deleted before the game was completed.
    public entry fun delete_ticket(ticket: Ticket) {
        let Ticket { id, game_id:  _, participant_index: _} = ticket;
        object::delete(id);
    }

    public entry fun delete_winner_ticket(ticket: WinnerTicket) {
        let WinnerTicket { id, game_id:  _} = ticket;
        object::delete(id);
    }

    public fun get_ticket_game_id(ticket: &Ticket): &ID {
        &ticket.game_id
    }

    public fun get_winner_ticket_game_id(ticket: &WinnerTicket): &ID {
        &ticket.game_id
    }

    fun closing_round(round: u64): u64 {
        round - 2
    }

    // Converts the first 16 bytes of rnd to a u128 number and outputs its modulo with input n.
    // Since n is u64, the output is at most 2^{-64} biased assuming rnd is uniformly random.
    fun safe_selection(n: u64, rnd: vector<u8>): u64 {
        assert!(vector::length(&rnd) >= 16, EInvalidSeed);
        let m: u128 = 0;
        let i = 0;
        while (i < 16) {
            m = m << 8;
            let curr_byte = *vector::borrow(&rnd, i);
            m = m + (curr_byte as u128);
            i = i + 1;
        };
        let n_128 = (n as u128);
        let module_128  = m % n_128;
        let res = (module_128 as u64);
        res
    }

    ////////////////////////
    // drand related code.

    // The public key of chain 8990e7a9aaed2ffed73dbd7092123d6f289930540d7651336225dc172e51b2ce.
    const DRAND_PUBLIC_KEY: vector<u8> =
        x"868f005eb8e6e4ca0a47c8a77ceaa5309a47978a7c71bc5cce96366b5d7a569937c529eeda66c7293784a9402801af31";

    fun verify_drand_signature(pk: vector<u8>, sig: vector<u8>, prev_sig: vector<u8>, round: u64): bool {
        // Convert round to a byte array in big-endian order.
        let round_bytes: vector<u8> = vector[0, 0, 0, 0, 0, 0, 0, 0];
        let i = 7;
        while (i > 0) {
            let curr_byte = round % 0x100;
            let curr_element = vector::borrow_mut(&mut round_bytes, i);
            *curr_element = (curr_byte as u8);
            round = round >> 8;
            i = i - 1;
        };

        // Compute sha256(prev_sig, round_bytes).
        vector::append(&mut prev_sig, round_bytes);
        let digest = sha2_256(prev_sig);

        // Verify the signature on the hash.
        bls12381::bls12381_min_pk_verify(&sig, &pk, &digest)
    }
}
