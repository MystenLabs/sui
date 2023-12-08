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
    use sui::bls12381::bls12381_min_pk_verify;
    use sui::hash::blake2b256;
    use scratch_off::math;
    use sui::table::{Self, Table};
    use sui::package;

    // --------------- Constants ---------------
    const EInvalidInputs: u64 = 0;
    const EInvalidBlsSig: u64 = 1;
    const ENoTicketsLeft: u64 = 2;
    const ENotAuthorizedEmployee: u64 = 3;

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

    struct PrizeStruct has store, drop {
        ticket_amount: u64,
        prize_value: u64,
    }

    struct Metadata has store, copy, drop {
        tickets_claimed: u64,
        amount_won: u64,
    }

    public fun tickets_claimed(metadata: &Metadata): u64 {
        metadata.tickets_claimed
    } 

    public fun amount_won(metadata: &Metadata): u64 {
        metadata.amount_won
    } 

    struct LeaderBoard has store {
        lowest_sui_won: u64,
        max_players: u64,
        leaderboard_players: vector<address>,
        leaderboard_player_metadata: Table<address, Metadata>,
    }

    /// Capability for the cap to manage the store id
    struct StoreCap has key, store {
        id: UID,
        store_id: ID
    }

    struct ConvenienceStore<phantom Asset> has key {
        id: UID,
        creator: address,
        /// Total prize pool available.
        prize_pool: Balance<Asset>,
        /// The vector of tickets and their corresponding prize winnings in the pool
        /// of winning tickets. (tickets left, prize awarded)
        winning_tickets: vector<PrizeStruct>,
        /// Total number of tickets originally available
        original_ticket_count: u64,
        /// Total number of tickets issued
        tickets_issued: u64,
        /// Tickets used in evaluation
        tickets_evaluated: u64,
        /// Mapping of amoung of sui won per address
        player_metadata: Table<address, Metadata>,
        /// Leaderboard for top 20 
        leaderboard: LeaderBoard,
        public_key: vector<u8>,
    }     

    public fun leaderboard<Asset>(store: &ConvenienceStore<Asset>): &LeaderBoard {
       &store.leaderboard 
    }

    public fun leaderboard_players(leaderboard: &LeaderBoard): vector<address> {
        leaderboard.leaderboard_players
    }

    public fun winning_tickets<Asset>(store: &ConvenienceStore<Asset>): &vector<PrizeStruct> {
        &store.winning_tickets
    }

    public fun table_contains_player(player_metadata: &Table<address, Metadata>, target_address: address): bool {
        table::contains(player_metadata, target_address)
    }

    public fun player_metadata<Asset>(store: &ConvenienceStore<Asset>): &Table<address, Metadata> {
        &store.player_metadata
    }

    public fun get_target_player_metadata(player_metadata: &Table<address, Metadata>, target_address: address): &Metadata {
        table::borrow(player_metadata, target_address)
    }

    public fun get_player_metadata_mut(player_metadata: &mut Table<address, Metadata>, target_address: address): &mut Metadata {
        table::borrow_mut(player_metadata, target_address)
    }

    public fun original_ticket_count<Asset>(store: &ConvenienceStore<Asset>): u64 {
        store.original_ticket_count
    }

    public fun tickets_left<Asset>(store: &ConvenienceStore<Asset>): u64 {
        store.original_ticket_count - store.tickets_evaluated
    }

    public fun tickets_issued<Asset>(store: &ConvenienceStore<Asset>): u64 {
        store.tickets_issued
    }

    public fun prize_pool_balance<Asset>(store: &ConvenienceStore<Asset>): u64 {
        balance::value(&store.prize_pool)
    }

    /// OTW to claim Publisher object, in order to create Display.
    struct GAME has drop {}

    /// We claim the cap for updating display
    fun init(otw: GAME, ctx: &mut TxContext){
        package::claim_and_keep(otw, ctx);
    }

    /// Initializes the store with all of the lottery tickets.
    /// We allow the user of the store.
    /// We purposely design the convenience store to be an owned object so that we
    /// don't need to make this in a shared format.
    public fun open_store<Asset>(
        coin: Coin<Asset>, 
        number_of_prizes: vector<u64>,
        value_of_prizes: vector<u64>,
        public_key: vector<u8>,
        max_players_in_leaderboard: u64,
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

        let new_store = ConvenienceStore<Asset> {
            id: object::new(ctx),
            creator: tx_context::sender(ctx),
            prize_pool,
            winning_tickets, 
            original_ticket_count: winning_ticket_count,
            tickets_issued: 0, 
            tickets_evaluated: 0,
            player_metadata: table::new(ctx),
            leaderboard: LeaderBoard {
                lowest_sui_won: 0,
                max_players: max_players_in_leaderboard,
                leaderboard_players: vector[],
                leaderboard_player_metadata: table::new(ctx),
            },
            public_key,
        };
        transfer::public_transfer(StoreCap {
            id: object::new(ctx),
            store_id: object::id(&new_store)
        }, tx_context::sender(ctx));

        transfer::share_object(new_store);
        coin
    }

    /// Initializes a ticket and sends it to someone.
    public fun send_ticket<Asset>(
        store_cap: &StoreCap,
        player: address,
        store: &mut ConvenienceStore<Asset>,
        ctx: &mut TxContext
    ) {
        assert!(store_cap.store_id == object::uid_to_inner(&store.id), ENotAuthorizedEmployee);
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
            bls12381_min_pk_verify(
                &bls_sig, &store.public_key, &msg_vec,
            ),
            EInvalidBlsSig
        );
        object::delete(id);

        // use the BLS to generate randomness
        let random_32b = blake2b256(&bls_sig);

        // Randomly pick a ticket from the prizes
        let target_index = math::get_random_u64_in_range(&random_32b, tickets_left<Asset>(store));
        let current_index = 0;
        // let current_prize = vector::pop_back(&mut store.winning_tickets);
        let winning_tickets_index = 0;
        // Identify the prize
        while (current_index < target_index) {
            let current_prize = vector::borrow(&store.winning_tickets, winning_tickets_index);
                current_index = current_index + current_prize.ticket_amount;
            if (current_index < target_index) {
                winning_tickets_index = winning_tickets_index + 1;
            };
        };
        // Update the ticket count in prizes and the total number
        let prize = vector::remove(&mut store.winning_tickets, winning_tickets_index);
        let prize_coin = coin::take(&mut store.prize_pool, prize.prize_value, ctx);
        prize.ticket_amount = prize.ticket_amount - 1;
        let value_won = prize.prize_value;
        if (prize.ticket_amount > 0) {
            vector::push_back(&mut store.winning_tickets, prize);
        };

        store.tickets_evaluated = store.tickets_evaluated + 1;
        transfer::public_transfer(prize_coin, player);
        // Update player mapping to metadata
        if (table_contains_player(&store.player_metadata, player)) {
            let player_metadata = get_player_metadata_mut(&mut store.player_metadata, player);
            player_metadata.tickets_claimed = player_metadata.tickets_claimed + 1;
            player_metadata.amount_won = player_metadata.amount_won + value_won;
        } else {
            table::add(&mut store.player_metadata, player, Metadata {
                tickets_claimed: 1, 
                amount_won: value_won
            });
        };

        // Update leaderboard 
        let leaderboard_updated = false;
        let player_metadata = get_target_player_metadata(&store.player_metadata, player);

        // Case where player is already on leaderboard
        if (table_contains_player(&store.leaderboard.leaderboard_player_metadata, player)) {
            table::remove(&mut store.leaderboard.leaderboard_player_metadata, player);
            table::add(&mut store.leaderboard.leaderboard_player_metadata, player, *player_metadata);
        } else {
            // Case where player is a new player
            if (store.leaderboard.lowest_sui_won < player_metadata.amount_won) {
                table::add(&mut store.leaderboard.leaderboard_player_metadata, player, *player_metadata);
                leaderboard_updated = true;
                vector::push_back(&mut store.leaderboard.leaderboard_players, player);
            };
        };

        // Update minimum amount_won in leaderboard
        // If the length of the leaderboard is greater than the minimum pop the lowest
        if (table::length(&store.leaderboard.leaderboard_player_metadata) > store.leaderboard.max_players) {
            let idx = 0;
            let min_index = 0;
            let current_player = vector::borrow(&store.leaderboard.leaderboard_players, idx);
            let data = table::borrow(&store.leaderboard.leaderboard_player_metadata, *current_player);
            let current_min = data.amount_won;
            let min_player = *current_player;
            idx = idx + 1;
            while (idx < store.leaderboard.max_players) {

                let current_player = vector::borrow(&store.leaderboard.leaderboard_players, idx);
                let data = table::borrow(&store.leaderboard.leaderboard_player_metadata, *current_player);
                if (current_min > data.amount_won) {
                    current_min = data.amount_won;
                    min_index = idx;
                    min_player = *current_player;
                };
                idx = idx + 1;
            };
            vector::remove(&mut store.leaderboard.leaderboard_players, min_index);
            table::remove(&mut store.leaderboard.leaderboard_player_metadata, min_player);
        };

        // Update the minimum sui won if leaderboard updated
        if (leaderboard_updated) {
            // Only need to update the minimum if we are at capacity
            if (table::length(&store.leaderboard.leaderboard_player_metadata) == store.leaderboard.max_players) {
                let idx = 0;
                let current_player = vector::borrow(&store.leaderboard.leaderboard_players, idx);
                let data = table::borrow(&store.leaderboard.leaderboard_player_metadata, *current_player);
                let current_min = data.amount_won;

                idx = idx + 1;
                while (idx < store.leaderboard.max_players) {
                    let current_player = vector::borrow(&store.leaderboard.leaderboard_players, idx);
                    let data = table::borrow(&store.leaderboard.leaderboard_player_metadata, *current_player);

                    if (current_min > data.amount_won) {
                        current_min = data.amount_won;
                    };
                    idx = idx + 1;
                };

                // Update the minimum
                store.leaderboard.lowest_sui_won = current_min;
            };
        };

        event::emit(DrawingResult<Asset> { 
            ticket_id,
            player: tx_context::sender(ctx),
            amount_won: value_won,
        });
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
        //     bls12381_min_pk_verify(
        //         &bls_sig, &store.public_key, &msg_vec,
        //     ),
        //     EInvalidBlsSig
        // );
        object::delete(id);

        // use the BLS to generate randomness
        let random_32b = blake2b256(&bls_sig);

        // Randomly pick a ticket from the prizes
        let target_index = math::get_random_u64_in_range(&random_32b, tickets_left<Asset>(store));
        let current_index = 0;
        // let current_prize = vector::pop_back(&mut store.winning_tickets);
        let winning_tickets_index = 0;
        // Identify the prize
        while (current_index < target_index) {
            let current_prize = vector::borrow(&store.winning_tickets, winning_tickets_index);
                current_index = current_index + current_prize.ticket_amount;
            if (current_index < target_index) {
                winning_tickets_index = winning_tickets_index + 1;
            };
        };
        // Update the ticket count in prizes and the total number
        let prize = vector::remove(&mut store.winning_tickets, winning_tickets_index);
        let prize_coin = coin::take(&mut store.prize_pool, prize.prize_value, ctx);
        prize.ticket_amount = prize.ticket_amount - 1;
        let value_won = prize.prize_value;
        if (prize.ticket_amount > 0) {
            vector::push_back(&mut store.winning_tickets, prize);
        };

        store.tickets_evaluated = store.tickets_evaluated + 1;
        transfer::public_transfer(prize_coin, player);
        // Update player mapping to metadata
        if (table_contains_player(&store.player_metadata, player)) {
            let player_metadata = get_player_metadata_mut(&mut store.player_metadata, player);
            player_metadata.tickets_claimed = player_metadata.tickets_claimed + 1;
            player_metadata.amount_won = player_metadata.amount_won + value_won;
        } else {
            table::add(&mut store.player_metadata, player, Metadata {
                tickets_claimed: 1, 
                amount_won: value_won
            });
        };

        // Update leaderboard 
        let leaderboard_updated = false;
        let player_metadata = get_target_player_metadata(&store.player_metadata, player);

        // Case where player is already on leaderboard
        if (table_contains_player(&store.leaderboard.leaderboard_player_metadata, player)) {
            table::remove(&mut store.leaderboard.leaderboard_player_metadata, player);
            table::add(&mut store.leaderboard.leaderboard_player_metadata, player, *player_metadata);
        } else {
            // Case where player is a new player
            if (store.leaderboard.lowest_sui_won < player_metadata.amount_won) {
                table::add(&mut store.leaderboard.leaderboard_player_metadata, player, *player_metadata);
                leaderboard_updated = true;
                vector::push_back(&mut store.leaderboard.leaderboard_players, player);
            };
        };

        // Update minimum amount_won in leaderboard
        // If the length of the leaderboard is greater than the minimum pop the lowest
        if (table::length(&store.leaderboard.leaderboard_player_metadata) > store.leaderboard.max_players) {
            let idx = 0;
            let min_index = 0;
            let current_player = vector::borrow(&store.leaderboard.leaderboard_players, idx);
            let data = table::borrow(&store.leaderboard.leaderboard_player_metadata, *current_player);
            let current_min = data.amount_won;
            let min_player = *current_player;
            idx = idx + 1;
            while (idx < store.leaderboard.max_players) {

                let current_player = vector::borrow(&store.leaderboard.leaderboard_players, idx);
                let data = table::borrow(&store.leaderboard.leaderboard_player_metadata, *current_player);
                if (current_min > data.amount_won) {
                    current_min = data.amount_won;
                    min_index = idx;
                    min_player = *current_player;
                };
                idx = idx + 1;
            };
            vector::remove(&mut store.leaderboard.leaderboard_players, min_index);
            table::remove(&mut store.leaderboard.leaderboard_player_metadata, min_player);
        };

        // Update the minimum sui won if leaderboard updated
        if (leaderboard_updated) {
            // Only need to update the minimum if we are at capacity
            if (table::length(&store.leaderboard.leaderboard_player_metadata) == store.leaderboard.max_players) {
                let idx = 0;
                let current_player = vector::borrow(&store.leaderboard.leaderboard_players, idx);
                let data = table::borrow(&store.leaderboard.leaderboard_player_metadata, *current_player);
                let current_min = data.amount_won;

                idx = idx + 1;
                while (idx < store.leaderboard.max_players) {
                    let current_player = vector::borrow(&store.leaderboard.leaderboard_players, idx);
                    let data = table::borrow(&store.leaderboard.leaderboard_player_metadata, *current_player);

                    if (current_min > data.amount_won) {
                        current_min = data.amount_won;
                    };
                    idx = idx + 1;
                };

                // Update the minimum
                store.leaderboard.lowest_sui_won = current_min;
            };
        };

        event::emit(DrawingResult<Asset> { 
            ticket_id,
            player: tx_context::sender(ctx),
            amount_won: value_won,
        });
    }
}