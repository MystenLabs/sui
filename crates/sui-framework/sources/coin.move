// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::coin {
    use sui::balance::{Self, Balance, Supply};
    use sui::id::{Self, VersionedID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use std::vector;

    /// A coin of type `T` worth `value`. Transferable and storable
    struct Coin<phantom T> has key, store {
        id: VersionedID,
        balance: Balance<T>
    }

    /// Capability allowing the bearer to mint and burn
    /// coins of type `T`. Transferable
    struct TreasuryCap<phantom T> has key, store {
        id: VersionedID,
        total_supply: Supply<T>
    }

    // === Supply <-> TreasuryCap morphing and accessors  ===

    /// Return the total number of `T`'s in circulation.
    public fun total_supply<T>(cap: &TreasuryCap<T>): u64 {
        balance::supply_value(&cap.total_supply)
    }

    /// Wrap a `Supply` into a transferable `TreasuryCap`.
    public fun treasury_from_supply<T>(total_supply: Supply<T>, ctx: &mut TxContext): TreasuryCap<T> {
        TreasuryCap { id: tx_context::new_id(ctx), total_supply }
    }

    /// Unwrap `TreasuryCap` getting the `Supply`.
    public fun treasury_into_supply<T>(treasury: TreasuryCap<T>): Supply<T> {
        let TreasuryCap { id, total_supply } = treasury;
        id::delete(id);
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
        Coin { id: tx_context::new_id(ctx), balance }
    }

    /// Destruct a Coin wrapper and keep the balance.
    public fun into_balance<T>(coin: Coin<T>): Balance<T> {
        let Coin { id, balance } = coin;
        id::delete(id);
        balance
    }

    /// Take a `Coin` worth of `value` from `Balance`.
    /// Aborts if `value > balance.value`
    public fun take<T>(
        balance: &mut Balance<T>, value: u64, ctx: &mut TxContext,
    ): Coin<T> {
        Coin {
            id: tx_context::new_id(ctx),
            balance: balance::split(balance, value)
        }
    }

    /// Put a `Coin<T>` to the `Balance<T>`.
    public fun put<T>(balance: &mut Balance<T>, coin: Coin<T>) {
        balance::join(balance, into_balance(coin));
    }

    // === Functionality for Coin<T> holders ===

    /// Send `c` to `recipient`
    public entry fun transfer<T>(c: Coin<T>, recipient: address) {
        transfer::transfer(c, recipient)
    }

    /// Transfer `c` to the sender of the current transaction
    public fun keep<T>(c: Coin<T>, ctx: &TxContext) {
        transfer(c, tx_context::sender(ctx))
    }

    /// Consume the coin `c` and add its value to `self`.
    /// Aborts if `c.value + self.value > U64_MAX`
    public entry fun join<T>(self: &mut Coin<T>, c: Coin<T>) {
        let Coin { id, balance } = c;
        id::delete(id);
        balance::join(&mut self.balance, balance);
    }

    /// Join everything in `coins` with `self`
    public entry fun join_vec<T>(self: &mut Coin<T>, coins: vector<Coin<T>>) {
        let i = 0;
        let len = vector::length(&coins);
        while (i < len) {
            let coin = vector::remove(&mut coins, i);
            join(self, coin);
            i = i + 1
        };
        // safe because we've drained the vector
        vector::destroy_empty(coins)
    }

    /// Destroy a coin with value zero
    public fun destroy_zero<T>(c: Coin<T>) {
        let Coin { id, balance } = c;
        id::delete(id);
        balance::destroy_zero(balance)
    }

    // === Registering new coin types and managing the coin supply ===

    /// Make any Coin with a zero value. Useful for placeholding
    /// bids/payments or preemptively making empty balances.
    public fun zero<T>(ctx: &mut TxContext): Coin<T> {
        Coin { id: tx_context::new_id(ctx), balance: balance::zero() }
    }

    /// Create a new currency type `T` as and return the `TreasuryCap`
    /// for `T` to the caller.
    /// NOTE: It is the caller's responsibility to ensure that
    /// `create_currency` can only be invoked once (e.g., by calling it from a
    /// module initializer with a `witness` object that can only be created
    /// in the initializer).
    public fun create_currency<T: drop>(
        witness: T,
        ctx: &mut TxContext
    ): TreasuryCap<T> {
        TreasuryCap {
            id: tx_context::new_id(ctx),
            total_supply: balance::create_supply(witness)
        }
    }

    /// Create a coin worth `value`. and increase the total supply
    /// in `cap` accordingly.
    public fun mint<T>(
        cap: &mut TreasuryCap<T>, value: u64, ctx: &mut TxContext,
    ): Coin<T> {
        Coin {
            id: tx_context::new_id(ctx),
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
        id::delete(id);
        balance::decrease_supply(&mut cap.total_supply, balance)
    }

    /// Give away the treasury cap to `recipient`
    public fun transfer_cap<T>(c: TreasuryCap<T>, recipient: address) {
        transfer::transfer(c, recipient)
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

    // === Test-only code ===

    #[test_only]
    /// Mint coins of any type for (obviously!) testing purposes only
    public fun mint_for_testing<T>(value: u64, ctx: &mut TxContext): Coin<T> {
        Coin { id: tx_context::new_id(ctx), balance: balance::create_for_testing(value) }
    }

    #[test_only]
    /// Destroy a `Coin` with any value in it for testing purposes.
    public fun destroy_for_testing<T>(self: Coin<T>): u64 {
        let Coin { id, balance } = self;
        id::delete(id);
        balance::destroy_for_testing(balance)
    }
}
