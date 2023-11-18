// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module scratch_off::game {

    use std::vector;
    use sui::coin::{Self, Coin};
    use sui::balance::{Self, Balance};
    use sui::object::{Self, UID, ID};
    use sui::tx_context::{Self, TxContext};
    use sui::dynamic_object_field as dof;
    use sui::transfer;
    use sui::event;
    use sui::bls12381::bls12381_min_pk_verify;
    use sui::hash::blake2b256;
    use scratch_off::math;

    // --------------- Constants ---------------
    const DEFAULT_TICKET_NUMBER: u64 = 100_000_000;

    const EInvalidInputs: u64 = 0;
    const EInvalidBlsSig: u64 = 1;

    // --------------- Events ---------------
    struct NewDrawing<phantom T> has copy, drop {
        ticket_id: ID,
        player: address,
    }

    struct Ticket has key, store {
		id: UID,
        /// Which store this ticket belongs to
        convenience_store_id: ID,
        /// Original receiver address, but on evaluation this is set to the ctx::sender addr
        player: address,
	}
    
	struct ConvenienceStore<phantom Asset> has key {
        id: UID,
        creator: address,
        /// This table is a list of prizes
        /// We will have each index correspond to a list of prizes i.e.
        /// 0: 50000 SUI
        /// 1: 100000 SUI
        /// 2: 200000 SUI
        prizes: vector<Balance<Asset>>,
        /// This is the prize odds for each of the prizes in the vector
        prize_odds: vector<u64>,
        prize_payouts: vector<u64>,
        public_key: vector<u8>,
	}

    /// Initializes the store with all of the lottery tickets.
    /// We allow the user of the store.
    /// We purposely design the convenience store to be an owned object so that we
    /// don't need to make this in a shared format.
    public fun open_store<Asset>(
        coin: Coin<Asset>, 
        prizes: vector<u64>,
        prize_odds: vector<u64>, 
        prize_payouts: vector<u64>,
        public_key: vector<u8>,
        ctx: &mut TxContext
    ): Coin<Asset> {
        let prizes_len = vector::length(&prizes);
        let odds_len = vector::length(&prizes);
        assert!(prizes_len == odds_len, EInvalidInputs);
        let coin_balance = coin::into_balance(coin);

        let prizes_balance = vector<Balance<Asset>>[];
        let idx = 0;
        while (idx < prizes_len) {
            let target_prize_balance = vector::pop_back(&mut prizes);
            let prize_balance = balance::split(&mut coin_balance, target_prize_balance);
            vector::push_back(&mut prizes_balance, prize_balance);
            idx = idx + 1;
        };

        let new_store = ConvenienceStore<Asset> {
            id: object::new(ctx),
            creator: tx_context::sender(ctx),
            prizes: prizes_balance,
            prize_odds,
            public_key,
            prize_payouts
        };

        // Publically share the object
        transfer::share_object(new_store);

        coin::from_balance(coin_balance, ctx)
    }

    /// Initializes a ticket and sends it to someone.
    public fun send_ticket<Asset>(
        target_address: address,
        store: &mut ConvenienceStore<Asset>,
        ctx: &mut TxContext
    ) {
        let ticket = Ticket {
            id: object::new(ctx),
            convenience_store_id: *object::uid_as_inner(&store.id),
            player: target_address
        };
        transfer::public_transfer(ticket, target_address);
    }

    /// Calls evaluate ticket by adding it as a dynamic field to the store
    /// We actually have a choice here to make users send tickets to our address and write
    /// the backend to listen to those, or we can list these objects as a dof in the shared_object
    public fun evaluate_ticket<Asset>(
        ticket: Ticket,
        store: &mut ConvenienceStore<Asset>,
        ctx: &mut TxContext
    ): ID {
        let ticket_id = object::uid_to_inner(&ticket.id);

        // Modify the ticket holder even if it got transfered so that the evaluation will
        // go to the player who sent this ticket
        ticket.player = tx_context::sender(ctx);
        dof::add(&mut store.id, ticket_id, ticket);

        event::emit(NewDrawing<Asset> { 
            ticket_id,
            player: tx_context::sender(ctx),
        });
        ticket_id
    }

    /// Reveals whether or not a user has won
    /// This function is called by the convenience store owner
    /// and we grab a prize from the store based on the number that was sent.
    public fun finish_evaluation<Asset>(
        ticket_id: ID,
        bls_sig: vector<u8>,
        store: &mut ConvenienceStore<Asset>,
        ctx: &mut TxContext
    ) {
        // get the game obj
        if (!game_exists<Asset>(store, ticket_id)) return;
        let ticket = dof::remove<ID, Ticket>(&mut store.id, ticket_id);
        let Ticket {
            id,
            convenience_store_id: _,
            player
        } = ticket;

        // verify BLS sig
        let msg_vec = object::uid_to_bytes(&id);
        assert!(
            bls12381_min_pk_verify(
                &bls_sig, &store.public_key, &msg_vec,
            ),
            EInvalidBlsSig
        );
        object::delete(id);

        // use the BLS to generate randomness
        let hashed_beacon = blake2b256(&bls_sig);
        let random_number = math::bytes_to_u256(&hashed_beacon);

        let result = math::get_result(random_number);

        // For loop through the prize_odds and if the result is in the range
        // we found a winner. Else this is a loser
        let index = 0;
        let bottom_range = 0;
        let is_winner = false;
        while (index < vector::length(&store.prize_odds)) {
            let current_prize_odds = vector::borrow(&store.prize_odds, index);
            let top_range = *current_prize_odds + bottom_range;
            if (result >= bottom_range && result < top_range) {
                is_winner = true;
                break
            };
            // Slide the probabilities
            bottom_range = top_range;
            index = index + 1;
        };

        // If we have a winner we find the index which is the winner and award that from
        // the prize pool. If the prize pool is insufficient, then we do not actually have 
        // a winner
        if (is_winner) {
            let prize_pool = vector::borrow_mut(&mut store.prizes, index);
            let prize_amount = vector::borrow(&store.prize_payouts, index);
            // This will fail if there isn't enough money in the prize pool
            let prize_coin = coin::take(prize_pool, *prize_amount, ctx);
            transfer::public_transfer(prize_coin, player);
        }
    }

    // --------------- House Accessors ---------------

    public fun public_key<T>(store: &ConvenienceStore<T>): vector<u8> {
        store.public_key
    }

    public fun game_exists<T>(store: &ConvenienceStore<T>, ticket_id: ID): bool {
        dof::exists_with_type<ID, Ticket>(&store.id, ticket_id)
    }
}