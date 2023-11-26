// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0


/// This is an example implementation of a scratch off game utilizing a BLS signature to generate randomness.
/// The game consists of a store which is a shared_object where users can go to evaluate their tickets.
/// A ticket object that is transferable and can be passed into be evaluated.
module scratch_off::game {

    use std::vector;
    use sui::coin::{Self, Coin};
    use sui::balance::{Self, Balance};
    use sui::object::{Self, UID, ID};
    use sui::tx_context::{Self, TxContext};
    use sui::dynamic_object_field as dof;
    use sui::transfer;
    use sui::event;
    use sui::bls12381::bls12381_min_sig_verify;
    use sui::hash::blake2b256;
    use scratch_off::math;

    // --------------- Constants ---------------
    const EInvalidInputs: u64 = 0;
    const EInvalidBlsSig: u64 = 1;
    const ENoTicketsLeft: u64 = 2;

    // --------------- Events ---------------
    struct NewDrawing<phantom T> has copy, drop {
        ticket_id: ID,
        player: address,
    }

    struct DrawingResult<phantom T> has copy, drop {
        ticket_id: ID,
        player: address,
        amount_won: u64,
    }

    struct Ticket has key, store {
        id: UID,
        /// Which store this ticket belongs to
        convenience_store_id: ID,
        /// Original receiver address, but on evaluation this is set to the ctx::sender addr
        player: address,
	}

    struct PrizeStruct has store {
        ticket_amount: u64,
        prize_value: u64,
    }

    struct ConvenienceStore<phantom Asset> has key {
        id: UID,
        creator: address,
        /// Total prize pool available.
        prize_pool: Balance<Asset>,
        /// The vector of tickets and their corresponding prize winnings in the pool
        /// of winning tickets. (tickets left, prize awarded)
        winning_tickets: vector<PrizeStruct>,
        /// Total number of losing tickets
        losing_tickets_left: u64,
        /// Total number of winning tickets
        winning_tickets_left: u64,
        /// Total number of tickets originally available
        original_ticket_count: u64,
        /// Total number of tickets issued
        tickets_issued: u64,
        public_key: vector<u8>,
    }     

    public fun total_tickets<Asset>(store: &ConvenienceStore<Asset>): u64 {
        store.winning_tickets_left + store.losing_tickets_left
    }

    public fun winning_tickets_left<Asset>(store: &ConvenienceStore<Asset>): u64 {
        store.winning_tickets_left
    }

    public fun losing_tickets_left<Asset>(store: &ConvenienceStore<Asset>): u64 {
        store.losing_tickets_left
    }

    public fun prize_pool_balance<Asset>(store: &ConvenienceStore<Asset>): u64 {
        balance::value(&store.prize_pool)
    }

    /// Initializes the store with all of the lottery tickets.
    /// We allow the user of the store.
    /// We purposely design the convenience store to be an owned object so that we
    /// don't need to make this in a shared format.
    public fun open_store<Asset>(
        coin: Coin<Asset>, 
        number_of_prizes: vector<u64>,
        value_of_prizes: vector<u64>,
        max_tickets_issued: u64,
        public_key: vector<u8>,
        ctx: &mut TxContext
    ): Coin<Asset> {
        let number_of_prizes_len = vector::length(&number_of_prizes);
        let value_of_prizes_len = vector::length(&value_of_prizes);
        assert!(number_of_prizes_len == value_of_prizes_len, EInvalidInputs);

        let winning_tickets = vector<PrizeStruct>[];
        let idx = 0;
        let prize_pool = balance::zero<Asset>();
        let winning_ticket_count = 0;

        while (idx < number_of_prizes_len) {
            let target_prize_amount = vector::pop_back(&mut number_of_prizes);
            let target_prize_value = vector::pop_back(&mut value_of_prizes);
            vector::push_back(&mut winning_tickets, PrizeStruct {
                ticket_amount: target_prize_amount,
                prize_value: target_prize_value
            });

            // prize_amount * prize_value to get the total we need.
            // Note that this is a u64 * u64 so we could have overflow but for the purpose
            // of this smart contract we do not need to consider this.
            let target_amount = target_prize_amount * target_prize_value;
            // Pull the required balance from the coin and stuff it into a balance.
            balance::join(&mut prize_pool, coin::into_balance(coin::split(&mut coin, target_amount, ctx)));
            winning_ticket_count = winning_ticket_count + target_prize_amount;
            idx = idx + 1;
        };

        assert!(max_tickets_issued == winning_ticket_count, EInvalidInputs);

        let new_store = ConvenienceStore<Asset> {
            id: object::new(ctx),
            creator: tx_context::sender(ctx),
            prize_pool,
            winning_tickets,
            losing_tickets_left: max_tickets_issued - winning_ticket_count,
            winning_tickets_left: winning_ticket_count,       
            original_ticket_count: max_tickets_issued,
            tickets_issued: 0, 
            public_key,
        };

        transfer::share_object(new_store);
        coin
    }

    /// Initializes a ticket and sends it to someone.
    /// TODO: decide how to do this besides sending a ticket
    /// TODO: add capability to this
    public fun send_ticket<Asset>(
        player: address,
        store: &mut ConvenienceStore<Asset>,
        ctx: &mut TxContext
    ) {
        assert!(store.tickets_issued < store.original_ticket_count, ENoTicketsLeft);
        store.tickets_issued = store.tickets_issued + 1;
        let ticket = Ticket {
            id: object::new(ctx),
            convenience_store_id: object::uid_to_inner(&store.id),
            player
        };
        transfer::public_transfer(ticket, player);
    }

    /// Calls evaluate ticket by adding it as a dynamic field to the store
    /// We actually have a choice here to make users send tickets to our address and write
    /// the backend to listen to those, or we can list these objects as a dof in the shared_object
    public fun evaluate_ticket<Asset>(
        ticket: Ticket,
        store: &mut ConvenienceStore<Asset>,
        ctx: &mut TxContext
    ): ID {
        assert!(store.tickets_issued <= store.original_ticket_count, ENoTicketsLeft);
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
    /// We also need to update the prize odds in this function and decrease prizes by 1
    public fun finish_evaluation<Asset>(
        ticket_id: ID,
        bls_sig: vector<u8>,
        store: &mut ConvenienceStore<Asset>,
        ctx: &mut TxContext
    ) {
        // get the game obj
        if (!ticket_exists<Asset>(store, ticket_id)) return;
        let Ticket {
            id,
            convenience_store_id: _,
            player
        } = dof::remove<ID, Ticket>(&mut store.id, ticket_id);

        // verify BLS sig
        let msg_vec = object::uid_to_bytes(&id);
        assert!(
            bls12381_min_sig_verify(
                &bls_sig, &store.public_key, &msg_vec,
            ),
            EInvalidBlsSig
        );
        object::delete(id);

        // use the BLS to generate randomness
        let random_32b = blake2b256(&bls_sig);

        let is_winner = math::should_draw_prize(
            &random_32b,
            store.winning_tickets_left,
            store.winning_tickets_left + store.losing_tickets_left
        );

        // If we have a winning ticket randomly draw a prize from the prize vector
        if (is_winner) {
            // Randomly pick a ticket from the prizes
            let target_index = math::get_random_u64_in_range(&random_32b, store.winning_tickets_left);
            let current_index = 0;
            // let current_prize = vector::pop_back(&mut store.winning_tickets);
            let winning_tickets_index = 0;
            // Identify the prize
            while (current_index < target_index) {
                let current_prize = vector::borrow(&store.winning_tickets, winning_tickets_index);
                current_index = current_index + current_prize.ticket_amount;
                winning_tickets_index = winning_tickets_index + 1;
            };

            // Update the ticket count in prizes and the total number
            let prize = vector::borrow_mut(&mut store.winning_tickets, winning_tickets_index);
            let prize_coin = coin::take(&mut store.prize_pool, prize.prize_value, ctx);
            prize.ticket_amount = prize.ticket_amount - 1;
            store.winning_tickets_left = store.winning_tickets_left - 1;
            transfer::public_transfer(prize_coin, player);

            event::emit(DrawingResult<Asset> { 
                ticket_id,
                player: tx_context::sender(ctx),
                amount_won: prize.prize_value,
            });
        } else {
            store.losing_tickets_left = store.losing_tickets_left - 1;
            event::emit(DrawingResult<Asset> { 
                ticket_id,
                player: tx_context::sender(ctx),
                amount_won: 0,
            });
        };
    }

    // --------------- House Accessors ---------------

    public fun public_key<T>(store: &ConvenienceStore<T>): vector<u8> {
        store.public_key
    }

    public fun ticket_exists<T>(store: &ConvenienceStore<T>, ticket_id: ID): bool {
        dof::exists_with_type<ID, Ticket>(&store.id, ticket_id)
    }

    // Tests
    #[test_only]
    public fun finish_evaluation_for_testing<Asset>(
        ticket_id: ID,
        bls_sig: vector<u8>,
        store: &mut ConvenienceStore<Asset>,
        ctx: &mut TxContext
    ) {
        // get the game obj
        if (!ticket_exists<Asset>(store, ticket_id)) return;
        let Ticket {
            id,
            convenience_store_id: _,
            player
        } = dof::remove<ID, Ticket>(&mut store.id, ticket_id);

        // verify BLS sig
        // let msg_vec = object::uid_to_bytes(&id);
        // assert!(
        //     bls12381_min_sig_verify(
        //         &bls_sig, &store.public_key, &msg_vec,
        //     ),
        //     EInvalidBlsSig
        // );
        object::delete(id);

        // use the BLS to generate randomness
        let random_32b = blake2b256(&bls_sig);

        let is_winner = math::should_draw_prize(
            &random_32b,
            store.winning_tickets_left,
            store.winning_tickets_left + store.losing_tickets_left
        );

        // If we have a winning ticket randomly draw a prize from the prize vector
        if (is_winner) {
            // Randomly pick a ticket from the prizes
            let target_index = math::get_random_u64_in_range(&random_32b, store.winning_tickets_left);
            let current_index = 0;
            // let current_prize = vector::pop_back(&mut store.winning_tickets);
            let winning_tickets_index = 0;
            // Identify the prize
            while (current_index < target_index) {
                let current_prize = vector::borrow(&store.winning_tickets, winning_tickets_index);
                current_index = current_index + current_prize.ticket_amount;
                winning_tickets_index = winning_tickets_index + 1;
            };

            // Update the ticket count in prizes and the total number
            let prize = vector::borrow_mut(&mut store.winning_tickets, winning_tickets_index);
            let prize_coin = coin::take(&mut store.prize_pool, prize.prize_value, ctx);
            prize.ticket_amount = prize.ticket_amount - 1;
            store.winning_tickets_left = store.winning_tickets_left - 1;
            transfer::public_transfer(prize_coin, player);

            event::emit(DrawingResult<Asset> { 
                ticket_id,
                player: tx_context::sender(ctx),
                amount_won: prize.prize_value,
            });
        } else {
            store.losing_tickets_left = store.losing_tickets_left - 1;
            event::emit(DrawingResult<Asset> { 
                ticket_id,
                player: tx_context::sender(ctx),
                amount_won: 0,
            });
        };
    }    
}