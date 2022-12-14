// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module contract::satoshi_flip_match {
    use std::option::{Self, Option};
    use std::hash::sha3_256;
    use std::vector;

    use sui::object::{Self, UID, ID};
    use sui::digest::{Self, Sha3256Digest};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer;

    const ENotGameHost: u64 = 0;
    const ENotGameGuesser: u64 = 1;
    const ENotCorrectSecret: u64 = 2;

    struct Match has key {
        id: UID,
        last_move_time: u64,
        host: address,
        guesser: address,
        hash: Option<Sha3256Digest>,
        guess: Option<u8>,
    }

    struct Outcome has key{
        id: UID,
        match_id: ID,
        winner: address,
        loser: address,
    }

    // accessors

    public fun get_host(match: &Match): address {
        match.host
    }

    public fun get_guesser(match: & Match): address {
        match.guesser
    }

    public fun get_guess(match: & Match): u8 {
        let guess = *option::borrow(&match.guess);
        guess
    }

    public fun get_winner(outcome: &Outcome): address {
        outcome.winner
    }


    // functions

    public fun create(host: address, guesser: address, ctx: &mut TxContext){
        let match = Match{
            id: object::new(ctx),
            last_move_time: tx_context::epoch(ctx),
            host,
            guesser,
            hash: option::none(),
            guess: option::none(),
        };

        transfer::transfer(match, host);
    }

    public entry fun set_hash(match: Match, hash: vector<u8>, ctx: &mut TxContext){
        // make sure that host is calling the function
        assert!(match.host == tx_context::sender(ctx), ENotGameHost);
        // update last move time with current epoch
        match.last_move_time = tx_context::epoch(ctx);
        // turn vector type into SHA3256-digest type
        let hash_value = digest::sha3_256_digest_from_bytes(hash);
        option::fill(&mut match.hash, hash_value);
        // transfer game to guesser so he can call create_guess afterwards
        let transfer_address = match.guesser;
        transfer::transfer(match, transfer_address);
    }

    public entry fun guess(match: Match, guess: u8, ctx: &mut TxContext){
        // make sure that guesser is calling the function
        assert!(match.guesser == tx_context::sender(ctx), ENotGameGuesser);
        // update last move time with current epoch
        match.last_move_time = tx_context::epoch(ctx);
        option::fill(&mut match.guess, guess);
        // transfer game back to host to call reveal afterwards and reveal the secret
        let transfer_address = match.host;
        transfer::transfer(match, transfer_address);
    }

    public entry fun reveal(match: Match, secret: vector<u8>, ctx: &mut TxContext){
        let Match {id, last_move_time: _, host, guesser, hash, guess} = match;
        assert!(host == tx_context::sender(ctx), ENotGameHost);
        let secret_hash = sha3_256(secret);
        let hash_value = option::borrow(&hash);
        assert!(secret_hash == digest::sha3_256_digest_to_bytes(hash_value), ENotCorrectSecret);
        let length = vector::length(&secret) - 1;
        let last_byte = vector::borrow(&secret, length);
        let player_guess = option::borrow(&guess);
        let winner = if ((*last_byte % 2) == *player_guess) guesser else host;
        let loser = if (winner == host) guesser else host;
        let outcome = Outcome {
            id: object::new(ctx),
            match_id: object::uid_to_inner(&id),
            winner,
            loser,
        };
        transfer::share_object(outcome);
        object::delete(id);
    }

}

// Question: should we update last move time in reveal function? probably no need since game will be destroyed.
// Maybe add last move time to outcome.