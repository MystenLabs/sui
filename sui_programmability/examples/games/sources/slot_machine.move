// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A betting game that depends on Sui randomness.
///
/// Anyone can create a new game by depositing SUIs as the initial balance.
///
/// Anyone can play the game by betting on X SUIs. They win X with probability 49% and loss the X SUIs otherwise.
/// A user calls bet() to play the game. The bet() function returns a round_id that can be used to complete the round
/// by calling complete().
///
/// Anyone (including the owner) can force completion of all rounds that are ready to be completed by calling
/// complete_ready().
///

module games::slot_machine {

    use std::vector;
    use sui::balance::{Self, Balance};
    use sui::coin::{Self, Coin, into_balance};
    use sui::object::{Self, UID};
    use sui::random::{Self, RandomnessRequest, RandomRounds};
    use sui::sui::SUI;
    use sui::transfer::{Self, share_object};
    use sui::tx_context::{Self, TxContext};

    const MAX_CONCURRENT_ROUNDS: u64 = 100;

    struct Game has key {
        id: UID,
        free_balance: Balance<SUI>,
        incomplete_rounds: vector<Round>, // TODO: ineffiecient, change to a list/queue
        round_id: u64,
        owner: address,
    }

    struct Round has store {
        round_id: u64,
        recepient: address,
        locked_balance: Balance<SUI>,
        randomness_request: RandomnessRequest,
    }

    public fun create(initial_balance: Coin<SUI>, ctx: &mut TxContext) {
        share_object(Game {
            id: object::new(ctx),
            free_balance: coin::into_balance(initial_balance),
            incomplete_rounds: vector::empty(),
            round_id: 0,
            owner: tx_context::sender(ctx),
        });
    }

    public fun withdraw(game: &mut Game, ctx: &mut TxContext) {
        assert!(tx_context::sender(ctx) == game.owner, 0); // TODO: error code
        assert!(vector::length(&game.incomplete_rounds) == 0, 0); // TODO: error code
        let coin_to_send = coin::from_balance(game.free_balance, ctx);
        transfer::public_transfer(coin_to_send, tx_context::sender(ctx));
    }

    public fun bet(game :&mut Game, bet: Coin<SUI>, rr: &RandomRounds, ctx: &mut TxContext): u64 {
        assert!(vector::length(&game.incomplete_rounds) < MAX_CONCURRENT_ROUNDS, 0);
        assert!(coin::value(&bet) <= balance::value(&game.free_balance), 0);

        let locked_balance = balance::split(&mut game.free_balance, coin::value(&bet));
        coin::put(&mut locked_balance, bet);
        let randomness_request = random::create_randomness_request(rr, ctx);

        let round = Round {
            round_id: game.round_id,
            recepient: tx_context::sender(ctx),
            locked_balance,
            randomness_request,

        };
        game.round_id = game.round_id + 1;
        vector::push_back(&mut game.incomplete_rounds, round);

        round.round_id
    }

    fun complete_ith(i: u64, game: &mut Game, rr: &RandomRounds, ctx: &mut TxContext) {
        let Round { round_id, recepient, locked_balance, randomness_request } = vector::remove(&mut game.incomplete_rounds, i);

        let gen = random::fulfill_and_create_generator(&randomness_request, rr);
        let random_number = random::generate_u8_in_range(&mut gen, 1, 100);
        let win = random_number < 50; // 49% chance of winning
        if (win) {
            let coin_to_send = coin::from_balance(locked_balance, ctx);
            transfer::public_transfer(coin_to_send, recepient);
            // emit event?
        } else {
            balance::join(&mut game.free_balance, locked_balance);
            // emit event?
        };
    }

    fun force_liquidate_ith(i: u64, game: &mut Game, rr: &RandomRounds, ctx: &mut TxContext) {
        let Round { round_id, recepient, locked_balance, randomness_request } = vector::remove(&mut game.incomplete_rounds, i);
        balance::join(&mut game.free_balance, locked_balance);
        // emit event?
    }

    public fun complete(round_id: u64, game: &mut Game, rr: &RandomRounds, ctx: &mut TxContext) {
        let i = 0;
        while (i < vector::length(&game.incomplete_rounds)) {
            let round = vector::borrow(&game.incomplete_rounds, i);
            if (round.round_id == round_id) {
                complete_ith(i, game, rr, ctx);
                return;
            };
            i = i + 1;
        };
        assert!(false, 0); // TODO: error code for no such round
    }

    // Executes all ongoing rounds that are ready to be completed, and force completion of old ones.
    public fun complete_ready(game: &mut Game, rr: &RandomRounds, ctx: &mut TxContext) {
        let i = 0;
        while (i < vector::length(&game.incomplete_rounds)) {
            let round = vector::borrow(&game.incomplete_rounds, i);
            if (random::is_available(&round.randomness_request, rr)) {
                complete_ith(i, game, rr, ctx);
                continue; // not incrementing i
            };
            if (random::is_too_old(&round.randomness_request, rr)) {
                force_liquidate_ith(i, game, rr, ctx);
            };
            i = i + 1;
        };
    }

    public fun get_balance(game: &Game): u64 {
        balance::value(&game.free_balance)
    }
}
