// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Utilities for `Coin` type particularly around managing groups of coins
module sui::coin_utils {
    use sui::transfer;
    use sui::coin::{Self, Coin};
    use sui::tx_context::{Self, TxContext};
    use std::vector;
    use sui::priority_queue;


    /// For when specifying a vector too big
    const EVecLenTooBig: u64 = 0;

    /// For when vector lengths mismatch
    const EVecLenMismatch: u64 = 1;

    const U64_MAX: u64 = 18446744073709551615;

    /// Join everything in `coins` with the final coin being the first in the vec
    /// We do this to reuse the ID of the first coin
    public fun join_vec_into_first<T>(coins: vector<Coin<T>>): Coin<T> {
        // We take a left and right side coin then merge them
        // Only take N-1 right side coins
        let len_all_but_first = vector::length(&coins) - 1;

        let i = len_all_but_first;
        // Pairwise merge in reverse order
        while (i >= 1) {
            // Right side coin
            let right_coin = vector::pop_back(&mut coins);
            // Left side coin
            let left_coin = vector::borrow_mut(&mut coins, i - 1);
            // Join in place of right side coin
            coin::join(left_coin, right_coin);
            i = i - 1;
        };

        let final = vector::pop_back(&mut coins);
        // safe because we've drained the vector
        vector::destroy_empty(coins);
        final
    }

    /// Transforms and transfers each specified amount to corresponding recipient in vector index 
    /// If we were unable to create enough coins, then we stop the transfer where we can.
    /// This will not abort if we cannot fulfill the transfers
    /// See `transform` function for explanation
    public entry fun transform_and_transfer_to_multiple_best_effort<T>(coins: vector<Coin<T>>, amounts: vector<u64>, recipients: vector<address>, ctx: &mut TxContext){
        assert!(vector::length(&amounts) == vector::length(&recipients), EVecLenMismatch);
        let output = transform_internal(coins, amounts, ctx);
        let coins_to_transfer_counter = 0;
        let rec_len = vector::length(&recipients);
        let out_len = vector::length(&output);
        let min = if (out_len > rec_len) rec_len else out_len;

        // For vector pop efficiency, transfer in reverse order via pop
        while (coins_to_transfer_counter < min) {
            transfer::transfer(vector::pop_back(&mut output), vector::pop_back(&mut recipients));
            coins_to_transfer_counter  = coins_to_transfer_counter + 1;
        };

        // If we ran out of coins to transfer, do nothing
        vector::destroy_empty(output);
    }

    /// Transforms and transfers each specified amount to corresponding recipient in vector index 
    /// If we were unable to create enough coins, then we abort 
    /// See `transform` function for explanation
    public entry fun transform_and_transfer_to_multiple_all_or_nothing<T>(coins: vector<Coin<T>>, amounts: vector<u64>, recipients: vector<address>, ctx: &mut TxContext){
        assert!(vector::length(&amounts) == vector::length(&recipients), EVecLenMismatch);
        let output = transform_internal(coins, amounts, ctx);
        let coins_to_transfer_counter = 0;
        let rec_len = vector::length(&recipients);
        let out_len = vector::length(&output);

        assert!(rec_len == out_len, EVecLenTooBig);

        // For vector pop efficiency, transfer in reverse order via pop
        while (coins_to_transfer_counter < rec_len) {
            transfer::transfer(vector::pop_back(&mut output), vector::pop_back(&mut recipients));
            coins_to_transfer_counter  = coins_to_transfer_counter + 1;
        };

        vector::destroy_empty(output);
    }

    /// Transforms and transfers to sender (self)
    /// See `transform` function for explanation
    public entry fun transform<T>(coins: vector<Coin<T>>, amounts: vector<u64>, ctx: &mut TxContext){
        transform_and_transfer_to_single(coins, amounts, tx_context::sender(ctx), ctx);
    }

    /// Transforms and transfers all coins to single recipient
    /// See `transform` function for explanation
    public entry fun transform_and_transfer_to_single<T>(coins: vector<Coin<T>>, amounts: vector<u64>, recipient: address, ctx: &mut TxContext){
        let output = transform_internal(coins, amounts, ctx);
        let output_coin_item_counter = 0;
        let len = vector::length(&output);

        // For vector pop efficiency, transfer in reverse order via pop
        while (output_coin_item_counter < len) {
            transfer::transfer(vector::pop_back(&mut output), recipient);
            output_coin_item_counter  = output_coin_item_counter + 1;
        };
        vector::destroy_empty(output);
    }


    /// Transforms a vector of coins to another with the specified amounts if possible
    /// We define `amount_to_fulfill` as total sum of values in `amounts`
    /// We define `amount_available` as total sum of values in `coins`
    /// This function also tries to avoid creating dust by merging smaller coins together where possible
    /// We greedily try to fulfil `amount` in the order specified
    /// Hence if amounts is [30, 50, 5], we will try to satisfy 30, then 50, then 5.
    /// This implies that for example if we had `amount_available` as 40, we will fulfill amounts[0], and part of amounts[1], but never amounts[2]
    /// Hence we will end up with [30, 10]. The last amount of 5 will not be reached.
    /// Depending on the `amount_to_fulfill` and amount_available, we may end up with more or less coins returned
    /// Case 1: Deficit
    /// If `amount_to_fulfill` > `amount_available`, we will not be able to fulfil all
    /// This means we will fulfil as many coins as possible but will not reach total_amount_requested.
    /// Hence len(output) <= len(amounts)
    /// Case 2: Surplus
    /// If `amount_to_fulfill` < `amount_available`, we will be able to fulfil all, and will have surplus coins
    /// Hence len(output) > len(amounts)
    /// Case 3: Exact
    /// If `amount_to_fulfill` == `amount_available`, we will be able to fulfil all, with no surplus coins
    /// Hence len(output) == len(amounts)
    public fun transform_internal<T>(coins: vector<Coin<T>>, amounts: vector<u64>, ctx: &mut TxContext): vector<Coin<T>> {
        let input_coins_len = vector::length(&coins);
        let amount_len = vector::length(&amounts);
        assert!(input_coins_len < (1<<63), EVecLenTooBig);

        if (amount_len == 0) {
            // Nothing to do, passthrough
            return coins
        };

        // Results of the transform
        let result = vector::empty<Coin<T>>();

        // Create entries and heapify the coin vector in increasing balance order
        let pq_entries = vector::empty();
        // Pop in reverse for perf of vec
        let input_coin_item_counter = 0u64;
        while (input_coin_item_counter < input_coins_len) {
            let coin = vector::pop_back(&mut coins);
            vector::push_back(&mut pq_entries, priority_queue::new_entry(coin::value(&coin), coin));
            input_coin_item_counter = input_coin_item_counter + 1;
        };

        // All the coins are used up. Must clean since Coin<T> has no `drop`
        vector::destroy_empty(coins);

        // Heapify in ascending order (smaller coins first)
        let min_pq = priority_queue::new(pq_entries, true);

        // For each amount, combine or split coins to create the valid coin
        let amount_item_counter = 0u64;
        // If we run out of target amounts or coins, we terminate
        while ((amount_item_counter < amount_len) && !priority_queue::empty(&min_pq)) {
            // Get the amount we need to create
            // Increase width to allow for temp overflow and calc ease
            let desired_amount = (*vector::borrow(&amounts, amount_item_counter) as u128);

            // Coins we will potentially merge into the desired amount
            let coins_to_be_merged = vector::empty<Coin<T>>();

            // Valid case for creating empty coins
            // Although we practice dust avoidance in this algo, if the user intentionally wants dust, we allow
            if (desired_amount == 0) {
                vector::push_back(&mut coins_to_be_merged, coin::zero(ctx));
            };

            // Amount we have so far from coins to merge
            // Using u128 for easier math
            let amount_so_far = 0u128;

            // Keep popping values from coins till we can meet the required amount
            // If we cannot meet the required amount, queue will be emptied and we will eventually terminate
            while (!priority_queue::empty(&min_pq) && (amount_so_far < desired_amount)) {
                let (coin_amt, coin_obj) = priority_queue::pop(&mut min_pq);
                let coin_amt = (coin_amt as u128);

                // If the new amount will push us over, we split the coin and take only what we need
                // Ensure no underflow or overflow 
                if (coin_amt + amount_so_far > desired_amount) {
                    let needed_difference = desired_amount - amount_so_far;
                    let surplus = coin_amt + amount_so_far - desired_amount;

                    // We want to include the smaller coin in our merge so we minimize dust
                    // Split off a coin with the larger difference
                    let amount_to_split_off = if (needed_difference > surplus) needed_difference else surplus;
                    let coin_to_heap = coin::take(coin::balance_mut(&mut coin_obj), (amount_to_split_off as u64), ctx);

                    let coin_to_merge = coin_obj;

                    // Put the larger coin back in the heap
                    priority_queue::insert(&mut min_pq, coin::value(&coin_to_heap), coin_to_heap);
                    // Coin amount has changed
                    coin_amt = (coin::value(&coin_to_merge) as u128);

                    // Track the coins used to reach this amount
                    vector::push_back(&mut coins_to_be_merged, coin_to_merge);
                } else {
                    // Merge this coin since it contributes to total amount
                    vector::push_back(&mut coins_to_be_merged, coin_obj);
                };

                // Incr the total amount seen
                amount_so_far = amount_so_far + coin_amt;
            };

            // Invariants
            // We must not exceed U64 if our logic is correct
            assert!(amount_so_far <= (U64_MAX as u128), 0);
            // We must not exceed the desired amount for this round
            assert!(amount_so_far <= desired_amount, 0);
            // There must be something to merge otherwise we wouldn't get here
            assert!(vector::length(&coins_to_be_merged) > 0, 0);

            // Merge all the coins we used to get the desired amount
            // Curr amount must be the amount needed or less
            let curr_coin = join_vec_into_first(coins_to_be_merged);

            // Save this
            vector::push_back(&mut result, curr_coin);
    
            amount_item_counter = amount_item_counter + 1;
        };

        // `result` now contains the desired coins
        // However there might be left over coins in the heap
        // We need to drain the items in the heap if left over
        let left_over = priority_queue::drain(min_pq);

        let len = vector::length(&left_over);

        let left_over_coin_item_counter = 0u64;
        while (left_over_coin_item_counter < len) {
            let coin = vector::pop_back(&mut left_over);
            vector::push_back(&mut result, coin);
            left_over_coin_item_counter = left_over_coin_item_counter + 1;
        };
        vector::destroy_empty(left_over);

        result
    }

    #[test_only]
    public fun transform_internal_for_testing<T>(coins: vector<Coin<T>>, amounts: vector<u64>, ctx: &mut TxContext): vector<Coin<T>> {
        transform_internal(coins, amounts, ctx)
    }
}
