// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module contract::tournament {
    // imports
    use sui::object::{Self, UID, ID};
    use std::vector;
    use sui::sui::SUI; // coin type
    use sui::balance::{Self, Balance};
    use sui::tx_context::{Self, TxContext};
    use sui::coin::{Self, Coin};
    use sui::transfer;
    use contract::satoshi_flip_match::{Self, Match, Outcome};
    use sui::dynamic_object_field as dof;

    // structs
    struct Tournament has key {
        id: UID,
        players: vector<address>,
        prize: Balance<SUI>,
        capacity: u64,
        status: u64, // Status -> 0: pending | 1: running | 2: finished
        round: u64,
        matches: vector<ID>,
    }

    // Player default entry fee in MIST
    const ENTRY_FEE: u64 = 10000;

    // Error codes
    const ENotEnoughMoney: u64 = 0;
    const EMaxPlayersReached: u64 = 1;
    const EPlayerNoExist: u64 = 2;
    const ETournamentNotFound: u64 = 3;
    const ECannotWithdraw: u64 = 4;
    const ECannotStartRound: u64 = 5;
    const ETournamentEnd: u64 = 6;
    const EPlayerAlreadyExists: u64 = 7;
    const EMatchNotFound: u64 = 8;

    // Tournament initialization. 
    // Player can initialize a new tournament and share it with other players. 
    // @param capacity: How many players are required to start the tournament. 
    // @param player_coin: Get first player's wallet balance to calculate whether 
    // he has an exact coin for the tournament fee or we need to split it.
    entry fun create(capacity: u64, player_coin: Coin<SUI>, ctx: &mut TxContext) {
        // Make sure player has given enough MIST
        assert!(coin::value(&player_coin) >= ENTRY_FEE, ENotEnoughMoney);

        // Give MIST back in case given fee is bigger or equal to entry_fee
        calc_player_change(&mut player_coin, ctx); 
    
        // Create a new tournament
        let tournament = Tournament {
            id: object::new(ctx),
            players: vector[tx_context::sender(ctx)],
            prize: coin::into_balance(player_coin),
            capacity,
            status: 0,
            round: 0,
            matches: vector::empty(),
        };

        // Make tournament shared obj so that every player can access it
        transfer::share_object(tournament);
    }

    entry fun join(tournament: &mut Tournament, player_coin: Coin<SUI>, ctx: &mut TxContext) {
        // Check if more players can join
        assert!(tournament.capacity < vector::length(&tournament.players), EMaxPlayersReached);

        // Check if player is already in tournament
        assert!(vector::contains(&tournament.players, tx_context::sender(ctx)), EPlayerAlreadyExists);

        // Make sure player has given enough mist
        assert!(coin::value(&player_coin) >= ENTRY_FEE, ENotEnoughMoney);

        // Determine if we should split player_coin and give back change
        calc_player_change(&mut player_coin, ctx); 

        vector::push_back(&mut tournament.players, tx_context::sender(ctx));
        balance::join(&mut tournament.prize, coin::into_balance(player_coin));
        
        // Once we reach capacity, start the tournament.
    }

    // If player's coin value is bigger than the required fee, then
    // calculate their change and split their coin so that player_coin == ENTRY_FEE
    fun calc_player_change(player_coin: &mut Coin<SUI>, ctx: &mut TxContext) {
        if(coin::value(player_coin) > ENTRY_FEE) {
            // Calculate how much change the player should get back
            let change = coin::value(player_coin) - ENTRY_FEE;
            // Split change into a new coin and transfer back to player
            let rebate = coin::split(player_coin, change, ctx);
            transfer::transfer(rebate, tx_context::sender(ctx));
        };
    }

    // Withdraw player attendance to tournament
    entry fun withdraw(tournament: &mut Tournament, ctx: &mut TxContext) {
        // Make sure tournament has not started :: state 0
        assert!(tournament.status == 0, ECannotWithdraw);

        // Find player's index in tournament's players vector
        let (player_exists, player_idx) = vector::index_of(&tournament.players, &tx_context::sender(ctx));
        
        // Make sure player had entered the tournament
        assert!(player_exists, EPlayerNoExist);

        // Remove player from tournament
        vector::remove(&mut tournament.players, player_idx);

        // Remove their coin from tournament's prize
        let player_payback = coin::take(&mut tournament.prize, ENTRY_FEE, ctx);

        // Transfer coin back to player
        transfer::transfer(player_payback, tx_context::sender(ctx));
    }

    // Start n-th round for tournament
    entry fun start_round(tournament: &mut Tournament, ctx: &mut TxContext) {
        let num_of_players = vector::length(&tournament.players);

        // Bail early if this is the last player
        assert!(num_of_players > 1, ETournamentEnd);

        // Make sure you are in the correct round
        assert!((tournament.capacity / (2^tournament.round)) == num_of_players, ECannotStartRound);

        let i = num_of_players;

        // Split players into matches of two-players
        while(i > 0) {
            // Assign last player to be the host
            let host = vector::pop_back(&mut tournament.players);

            // Assign second to last player to be the guesser
            let guesser = vector::pop_back(&mut tournament.players);
            
            // Create a match for current pair of host-guesser
            let match = satoshi_flip_match::create(host, guesser, ctx);
            let match_id = satoshi_flip_match::id(match);

            // Include match to tournament
            vector::add(&mut tournament.matches, match_id);

            // Transfer match to host
            transfer::transfer(match, host);

            i = i - 2;
        };
    }

    // Get all active matches and update tournament's players with winners
    fun end_round(tournament: &mut Tournament, match: Match, ctx: &mut TxContext) {
        // Get matches length from tournament
        let i = vector::length(&tournament.matches);

        while(i > 0) {
            // Remove match from tournament and get their ID
            let match_id = vector::pop_back(&mut tournament.matches);

            // Make sure that match is part of the tournament
            assert!(dof::exists_(&tournament, match_id), EMatchNotFound);

            // Find match dynamic object field in tournament
            let match = dof::remove(tournament, match_id);

            // Get match winner and add them to tournament's players
            let winner = satoshi_flip_match::get_winner(&match);
            vector::push_back(&mut tournament.players, winner);

            i = i - 1;
        };

        // Increment round counter for next iteration
        tournament.round = tournament.round + 1;
    }
}

