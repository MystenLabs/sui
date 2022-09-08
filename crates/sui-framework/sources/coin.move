// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Defines the `Coin` type - platform wide representation of fungible
/// tokens and coins. `Coin` can be described as a secure wrapper around
/// `Balance` type.
module sui::coin {
    use sui::balance::{Self, Balance, Supply};
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use std::vector;
    use sui::priority_queue;

    /// For when a type passed to create_supply is not a one-time witness.
    const EBadWitness: u64 = 0;

    /// For when invalid arguments are passed to a function.
    const EInvalidArg: u64 = 1;

    /// For when trying to split a coin more times than its balance allows.
    const ENotEnough: u64 = 2;

    /// For when specifying a vector too big
    const EVecLenTooBig: u64 = 3;

    /// A coin of type `T` worth `value`. Transferable and storable
    struct Coin<phantom T> has key, store {
        id: UID,
        balance: Balance<T>
    }

    /// Capability allowing the bearer to mint and burn
    /// coins of type `T`. Transferable
    struct TreasuryCap<phantom T> has key, store {
        id: UID,
        total_supply: Supply<T>
    }

    // === Supply <-> TreasuryCap morphing and accessors  ===

    /// Return the total number of `T`'s in circulation.
    public fun total_supply<T>(cap: &TreasuryCap<T>): u64 {
        balance::supply_value(&cap.total_supply)
    }

    /// Unwrap `TreasuryCap` getting the `Supply`.
    ///
    /// Operation is irreversible. Supply cannot be converted into a `TreasuryCap` due
    /// to different security guarantees (TreasuryCap can be created only once for a type)
    public fun treasury_into_supply<T>(treasury: TreasuryCap<T>): Supply<T> {
        let TreasuryCap { id, total_supply } = treasury;
        object::delete(id);
        total_supply
    }

    /// Get immutable reference to the treasury's `Supply`.
    public fun supply<T>(treasury: &mut TreasuryCap<T>): &Supply<T> {
        &treasury.total_supply
    }

    /// Get mutable reference to the treasury's `Supply`.
    public fun supply_mut<T>(treasury: &mut TreasuryCap<T>): &mut Supply<T> {
        &mut treasury.total_supply
    }

    // === Balance <-> Coin accessors and type morphing ===

    /// Public getter for the coin's value
    public fun value<T>(self: &Coin<T>): u64 {
        balance::value(&self.balance)
    }

    /// Get immutable reference to the balance of a coin.
    public fun balance<T>(coin: &Coin<T>): &Balance<T> {
        &coin.balance
    }

    /// Get a mutable reference to the balance of a coin.
    public fun balance_mut<T>(coin: &mut Coin<T>): &mut Balance<T> {
        &mut coin.balance
    }

    /// Wrap a balance into a Coin to make it transferable.
    public fun from_balance<T>(balance: Balance<T>, ctx: &mut TxContext): Coin<T> {
        Coin { id: object::new(ctx), balance }
    }

    /// Destruct a Coin wrapper and keep the balance.
    public fun into_balance<T>(coin: Coin<T>): Balance<T> {
        let Coin { id, balance } = coin;
        object::delete(id);
        balance
    }

    /// Take a `Coin` worth of `value` from `Balance`.
    /// Aborts if `value > balance.value`
    public fun take<T>(
        balance: &mut Balance<T>, value: u64, ctx: &mut TxContext,
    ): Coin<T> {
        Coin {
            id: object::new(ctx),
            balance: balance::split(balance, value)
        }
    }

    /// Put a `Coin<T>` to the `Balance<T>`.
    public fun put<T>(balance: &mut Balance<T>, coin: Coin<T>) {
        balance::join(balance, into_balance(coin));
    }

    // === Functionality for Coin<T> holders ===

    /// Transfer `c` to the sender of the current transaction
    public fun keep<T>(c: Coin<T>, ctx: &TxContext) {
        transfer::transfer(c, tx_context::sender(ctx))
    }

    /// Consume the coin `c` and add its value to `self`.
    /// Aborts if `c.value + self.value > U64_MAX`
    public entry fun join<T>(self: &mut Coin<T>, c: Coin<T>) {
        let Coin { id, balance } = c;
        object::delete(id);
        balance::join(&mut self.balance, balance);
    }

    /// Join everything in `coins` with `self`
    public entry fun join_vec<T>(self: &mut Coin<T>, coins: vector<Coin<T>>) {
        let i = 0;
        let len = vector::length(&coins);
        while (i < len) {
            let coin = vector::pop_back(&mut coins);
            join(self, coin);
            i = i + 1
        };
        // safe because we've drained the vector
        vector::destroy_empty(coins)
    }

    /// Destroy a coin with value zero
    public fun destroy_zero<T>(c: Coin<T>) {
        let Coin { id, balance } = c;
        object::delete(id);
        balance::destroy_zero(balance)
    }

    // === Registering new coin types and managing the coin supply ===

    /// Make any Coin with a zero value. Useful for placeholding
    /// bids/payments or preemptively making empty balances.
    public fun zero<T>(ctx: &mut TxContext): Coin<T> {
        Coin { id: object::new(ctx), balance: balance::zero() }
    }

    /// Create a new currency type `T` as and return the `TreasuryCap` for
    /// `T` to the caller. Can only be called with a `one-time-witness`
    /// type, ensuring that there's only one `TreasuryCap` per `T`.
    public fun create_currency<T: drop>(
        witness: T,
        ctx: &mut TxContext
    ): TreasuryCap<T> {
        // Make sure there's only one instance of the type T
        assert!(sui::types::is_one_time_witness(&witness), EBadWitness);

        TreasuryCap {
            id: object::new(ctx),
            total_supply: balance::create_supply(witness)
        }
    }

    /// Create a coin worth `value`. and increase the total supply
    /// in `cap` accordingly.
    public fun mint<T>(
        cap: &mut TreasuryCap<T>, value: u64, ctx: &mut TxContext,
    ): Coin<T> {
        Coin {
            id: object::new(ctx),
            balance: balance::increase_supply(&mut cap.total_supply, value)
        }
    }

    /// Mint some amount of T as a `Balance` and increase the total
    /// supply in `cap` accordingly.
    /// Aborts if `value` + `cap.total_supply` >= U64_MAX
    public fun mint_balance<T>(
        cap: &mut TreasuryCap<T>, value: u64
    ): Balance<T> {
        balance::increase_supply(&mut cap.total_supply, value)
    }

    /// Destroy the coin `c` and decrease the total supply in `cap`
    /// accordingly.
    public fun burn<T>(cap: &mut TreasuryCap<T>, c: Coin<T>): u64 {
        let Coin { id, balance } = c;
        object::delete(id);
        balance::decrease_supply(&mut cap.total_supply, balance)
    }

    // === Entrypoints ===

    /// Mint `amount` of `Coin` and send it to `recipient`. Invokes `mint()`.
    public entry fun mint_and_transfer<T>(
        c: &mut TreasuryCap<T>, amount: u64, recipient: address, ctx: &mut TxContext
    ) {
        transfer::transfer(mint(c, amount, ctx), recipient)
    }

    /// Burn a Coin and reduce the total_supply. Invokes `burn()`.
    public entry fun burn_<T>(c: &mut TreasuryCap<T>, coin: Coin<T>) {
        burn(c, coin);
    }

    /// Send `amount` units of `c` to `recipient
    /// Aborts with `EVALUE` if `amount` is greater than or equal to `amount`
    public entry fun split_and_transfer<T>(
        c: &mut Coin<T>, amount: u64, recipient: address, ctx: &mut TxContext
    ) {
        transfer::transfer(take(&mut c.balance, amount, ctx), recipient)
    }

    /// Split coin `self` to two coins, one with balance `split_amount`,
    /// and the remaining balance is left is `self`.
    public entry fun split<T>(self: &mut Coin<T>, split_amount: u64, ctx: &mut TxContext) {
        transfer::transfer(
            take(&mut self.balance, split_amount, ctx),
            tx_context::sender(ctx)
        )
    }

    /// Split coin `self` into `n` coins with equal balances. If the balance is
    /// not evenly divisible by `n`, the remainder is left in `self`. Return
    /// newly created coins.
    public fun split_n_to_vec<T>(self: &mut Coin<T>, n: u64, ctx: &mut TxContext): vector<Coin<T>> {
        assert!(n > 0, EInvalidArg);
        assert!(n <= balance::value(&self.balance), ENotEnough);
        let vec = vector::empty<Coin<T>>();
        let i = 0;
        let split_amount = balance::value(&self.balance) / n;
        while (i < n - 1) {
            vector::push_back(&mut vec, take(&mut self.balance, split_amount, ctx));
            i = i + 1;
        };
        vec
    }

    /// Split coin `self` into `n` coins with equal balances. If the balance is
    /// not evenly divisible by `n`, the remainder is left in `self`.
    public entry fun split_n<T>(self: &mut Coin<T>, n: u64, ctx: &mut TxContext) {
        let vec: vector<Coin<T>> = split_n_to_vec(self, n, ctx);
        let i = 0;
        let len = vector::length(&vec);
        while (i < len) {
            transfer::transfer(vector::pop_back(&mut vec), tx_context::sender(ctx));
            i = i + 1;
        };
        vector::destroy_empty(vec);
    }

    /// Split coin `self` into multiple coins, each with balance specified
    /// in `split_amounts`. Remaining balance is left in `self`.
    public entry fun split_vec<T>(self: &mut Coin<T>, split_amounts: vector<u64>, ctx: &mut TxContext) {
        let i = 0;
        let len = vector::length(&split_amounts);
        while (i < len) {
            split(self, *vector::borrow(&split_amounts, i), ctx);
            i = i + 1;
        };
    }


    // Transforms a vector of coins to another with the specified amounts
    // 1. Heapify the coin list in ascending order
    public fun transform<T>(coins: vector<Coin<T>>, amounts: vector<u64>, ctx: &mut TxContext): vector<Coin<T>> {
        let input_coins_len = vector::length(&coins);
        let amount_len = vector::length(&amounts);
        assert!(input_coins_len < (1<<63), EVecLenTooBig);

        if (amount_len == 0) {
            // Nothing to do
            return coins
        };

        let result = vector::empty<Coin<T>>();

        // Create entries and heapify the coin vector in increasing balance order
        let pq_entries = vector::empty();
        // Do in reverse for perf of vec pop
        let i = 0u64;
        while (i < input_coins_len) {
            let coin = vector::pop_back(&mut coins);
            vector::push_back(&mut pq_entries, priority_queue::new_entry(value(&coin), coin));
            i = i + 1;
        };
        vector::destroy_empty(coins);

        // Heapify in ascending order
        let min_pq = priority_queue::new(pq_entries, true);

        // For each amount, combine or split coins to create the valid coin
        let i = 0u64;
        while ((i < amount_len) && !priority_queue::empty(&min_pq)) {
            // Get the amount we need to create
            let desired_amount = vector::borrow(&amounts, i);
            // Increase width to allow for temp overflow and calc ease
            let desired_amount = ((*desired_amount) as u128);

            // Create the zero coin where we will put the amount
            let curr_coin = zero(ctx);

            // Coins to modify
            let coins_to_be_merged = vector::empty<Coin<T>>();

            // Amount we have so far from coins to modify
            // Using u128 as it might temporarily overflow
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
                    let coin_to_heap = take(&mut coin_obj.balance, (amount_to_split_off as u64), ctx);

                    let coin_to_merge = coin_obj;

                    // Put the larger coin back in the heap
                    priority_queue::insert(&mut min_pq, value(&coin_to_heap), coin_to_heap);
                    // Coin amount has changed
                    coin_amt = (value(&coin_to_merge) as u128);

                    // Track the coins used to reach this amount
                    vector::push_back(&mut coins_to_be_merged, coin_to_merge);
                } else {
                    // Merge this coin since it contributes to total amount
                    vector::push_back(&mut coins_to_be_merged, coin_obj);
                };

                // Incr the total amount seen
                amount_so_far = amount_so_far + coin_amt;
            };
            assert!(amount_so_far <= desired_amount, 0);

            // Merge all the coins we used to get the desired amount
            // Curr amount must be the amount needed or less
            join_vec(&mut curr_coin, coins_to_be_merged);
            // Save this
            vector::push_back(&mut result, curr_coin);
    
            i = i + 1;
        };

        // `result` now contains the desired coins
        // However there might be left over coins in the heap
        // We need to drain the items in the heap if left over
        let left_over = priority_queue::drain(min_pq);

        let len = vector::length(&left_over);

        let i = 0u64;
        while (i < len) {
            let coin = vector::pop_back(&mut left_over);
            vector::push_back(&mut result, coin);
            i = i + 1;
        };
        vector::destroy_empty(left_over);

        result
    }

    // === Test-only code ===

    #[test_only]
    /// Mint coins of any type for (obviously!) testing purposes only
    public fun mint_for_testing<T>(value: u64, ctx: &mut TxContext): Coin<T> {
        Coin { id: object::new(ctx), balance: balance::create_for_testing(value) }
    }

    #[test_only]
    /// Destroy a `Coin` with any value in it for testing purposes.
    public fun destroy_for_testing<T>(self: Coin<T>): u64 {
        let Coin { id, balance } = self;
        object::delete(id);
        balance::destroy_for_testing(balance)
    }
}
