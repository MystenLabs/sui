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

    // --------------- Constants ---------------
    const DEFAULT_TICKET_NUMBER: u64 = 100_000_000;

    const EInvalidInputs: u64 = 1;


    struct Ticket has key, store {
		id: UID,
        /// Which store this ticket belongs to
        convenience_store_id: ID,
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
	}

    /// Initializes the store with all of the lottery tickets.
    /// We allow the user of the store.
    /// We purposely design the convenience store to be an owned object so that we
    /// don't need to make this in a shared format.
    public fun open_store<Asset>(
        coin: Coin<Asset>, 
        prizes: vector<u64>,
        prize_odds: vector<u64>, 
        ctx: &mut TxContext
    ): (Coin<Asset>, ConvenienceStore<Asset>) {
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
            prize_odds
        };

        (coin::from_balance(coin_balance, ctx), new_store)
    }

    /// Initializes a ticket and sends it to someone.
    public fun send_ticket<Asset>(
        target_address: address,
        store: &mut ConvenienceStore<Asset>,
        ctx: &mut TxContext
    ) {

        let ticket = Ticket {
            id: object::new(ctx),
            convenience_store_id: *object::uid_as_inner(&store.id)
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
    ) {
        /// dof::add(store, ticket_id, ticket)
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

    }
}