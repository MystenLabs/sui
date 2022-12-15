// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module contract::satoshi_flip_match {
    use std::option::{Self, Option};
    use std::hash::sha3_256;
    use std::vector;

    use sui::object::{Self, UID, ID};
    use sui::digest::{Self, Sha3256Digest};
    use sui::tx_context::{Self, TxContext};

    const ENotMatchHost: u64 = 0;
    const ENotMatchGuesser: u64 = 1;
    const ENotCorrectSecret: u64 = 2;
    const EMatchNotEnded: u64 = 3;
    const EGuessNotSet: u64 = 4;

    struct Match has key {
        id: UID,
        last_move_time: u64,
        host: address,
        guesser: address,
        hash: Option<Sha3256Digest>,
        guess: Option<u8>,
        secret: Option<vector<u8>>,
        round: u64,
        winner: Option<address>,
    }

    // accessors

    public fun id(match: &Match): ID{
        let id = object::uid_to_inner(&match.id);
        id
    }

    public fun host(match: &Match): address {
        match.host
    }

    public fun guesser(match: &Match): address {
        match.guesser
    }

    public fun guess(match: &Match): u8 {
        assert!(option::is_some(&match.guess), EGuessNotSet);
        let guess = *option::borrow(&match.guess);
        guess
    }

    public fun winner(match: &Match): address{
        assert!(option::is_some(&match.winner), EMatchNotEnded);
        let winner = *option::borrow(&match.winner);
        winner
    }

    public fun id(match: &Match): UID {
        match.id
    }

    // functions

    public fun create(host: address, guesser: address, round: u64, ctx: &mut TxContext): Match{
        let match = Match{
            id: object::new(ctx),
            last_move_time: tx_context::epoch(ctx),
            host,
            guesser,
            hash: option::none(),
            guess: option::none(),
            secret: option::none(),
            round,
            winner: option::none(),
        };
        match
    }

    public fun set_hash(match: Match, hash: vector<u8>, ctx: &mut TxContext): Match{
        // make sure that host is calling the function
        assert!(match.host == tx_context::sender(ctx), ENotMatchHost);
        // update last move time with current epoch
        match.last_move_time = tx_context::epoch(ctx);
        // turn vector type into SHA3256-digest type
        let hash_value = digest::sha3_256_digest_from_bytes(hash);
        // add hash value to match
        option::fill(&mut match.hash, hash_value);
        match
    }

    public fun set_guess(match: Match, guess: u8, ctx: &mut TxContext): Match{
        // make sure that guesser is calling the function
        assert!(match.guesser == tx_context::sender(ctx), ENotMatchGuesser);
        // update last move time with current epoch
        match.last_move_time = tx_context::epoch(ctx);
        // add guess to match
        option::fill(&mut match.guess, guess);
        match
    }

    public fun reveal(match: Match, secret: vector<u8>): Match {
        // assert!(match.host == tx_context::sender(ctx), ENotMatchHost);
        let secret_hash = sha3_256(secret);
        let hash_value = option::borrow(&match.hash);
        // make sure match.hash is hash of secret
        assert!(secret_hash == digest::sha3_256_digest_to_bytes(hash_value), ENotCorrectSecret);
        // take last byte
        let length = vector::length(&secret) - 1;
        let last_byte = vector::borrow(&secret, length);
        // take player guess
        let player_guess = option::borrow(&match.guess);
        // decide on winner
        let winner = if ((*last_byte % 2) == *player_guess) match.guesser else match.host;
        // add winner to match
        option::fill(&mut match.winner, winner); 
        match   
    }

}