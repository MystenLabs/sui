// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module Sui::Coin {
    use Sui::Balance::{Self, Balance};
    use Sui::ID::{Self, VersionedID};
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};
    use Std::Vector;

    /// A coin of type `T` worth `value`. Transferrable
    struct Coin<phantom T> has key, store {
        id: VersionedID,
        balance: Balance<T>
    }

    /// Capability allowing the bearer to mint and burn
    /// coins of type `T`. Transferrable
    struct TreasuryCap<phantom T> has key, store {
        id: VersionedID,
        total_supply: u64
    }

    /// Trying to withdraw N from a coin with value < N
    const EVALUE: u64 = 0;
    /// Trying to destroy a coin with a nonzero value
    const ENONZERO: u64 = 0;

    // === Balance accessors and type morphing methods ===

    public fun balance<T>(coin: &Coin<T>): &Balance<T> {
        &coin.balance
    }

    public fun balance_mut<T>(coin: &mut Coin<T>): &mut Balance<T> {
        &mut coin.balance
    }

    public fun wrap<T>(balance: Balance<T>, ctx: &mut TxContext): Coin<T> {
        Coin { id: TxContext::new_id(ctx), balance }
    }

    public fun unwrap<T>(coin: Coin<T>): Balance<T> {
        let Coin { id, balance } = coin;
        ID::delete(id);
        balance
    }

    // === Functionality for Coin<T> holders ===

    /// Send `c` to `recipient`
    public fun transfer<T>(c: Coin<T>, recipient: address) {
        Transfer::transfer(c, recipient)
    }

    /// Transfer `c` to the sender of the current transaction
    public fun keep<T>(c: Coin<T>, ctx: &TxContext) {
        transfer(c, TxContext::sender(ctx))
    }

    /// Consume the coin `c` and add its value to `self`.
    /// Aborts if `c.value + self.value > U64_MAX`
    public fun join<T>(self: &mut Coin<T>, c: Coin<T>) {
        let Coin { id, balance } = c;
        ID::delete(id);
        Balance::join(&mut self.balance, balance);
    }

    /// Join everything in `coins` with `self`
    public fun join_vec<T>(self: &mut Coin<T>, coins: vector<Coin<T>>) {
        let i = 0;
        let len = Vector::length(&coins);
        while (i < len) {
            let coin = Vector::remove(&mut coins, i);
            join(self, coin);
            i = i + 1
        };
        // safe because we've drained the vector
        Vector::destroy_empty(coins)
    }

    /// Subtract `value` from `self` and create a new coin
    /// worth `value` with ID `id`.
    /// Aborts if `value > self.value`
    public fun withdraw<T>(
        self: &mut Coin<T>, value: u64, ctx: &mut TxContext,
    ): Coin<T> {
        let balance = Balance::split(&mut self.balance, value);
        Coin { id: TxContext::new_id(ctx), balance }
    }

    /// Public getter for the coin's value
    public fun value<T>(self: &Coin<T>): u64 {
        Balance::value(&self.balance)
    }

    /// Destroy a coin with value zero
    public fun destroy_zero<T>(c: Coin<T>) {
        let Coin { id, balance } = c;
        ID::delete(id);
        Balance::destroy_empty(balance);
    }

    // === Registering new coin types and managing the coin supply ===

    /// Make any Coin with a zero value. Useful for placeholding
    /// bids/payments or preemptively making empty balances.
    public fun zero<T>(ctx: &mut TxContext): Coin<T> {
        Coin { id: TxContext::new_id(ctx), balance: Balance::empty() }
    }

    /// Create a new currency type `T` as and return the `TreasuryCap`
    /// for `T` to the caller.
    /// NOTE: It is the caller's responsibility to ensure that
    /// `create_currency` can only be invoked once (e.g., by calling it from a
    /// module initializer with a `witness` object that can only be created
    /// in the initializer).
    public fun create_currency<T: drop>(
        _witness: T,
        ctx: &mut TxContext
    ): TreasuryCap<T> {
        TreasuryCap { id: TxContext::new_id(ctx), total_supply: 0 }
    }

    /// Create a coin worth `value`. and increase the total supply
    /// in `cap` accordingly
    /// Aborts if `value` + `cap.total_supply` >= U64_MAX
    public fun mint<T>(
        value: u64, cap: &mut TreasuryCap<T>, ctx: &mut TxContext,
    ): Coin<T> {
        cap.total_supply = cap.total_supply + value;
        Coin { id: TxContext::new_id(ctx), balance: Balance::create(value) }
    }

    /// Destroy the coin `c` and decrease the total supply in `cap`
    /// accordingly.
    public fun burn<T>(c: Coin<T>, cap: &mut TreasuryCap<T>) {
        let Coin { id, balance } = c;
        let value = Balance::destroy<T>(balance);
        ID::delete(id);
        cap.total_supply = cap.total_supply - value
    }

    /// Return the total number of `T`'s in circulation
    public fun total_supply<T>(cap: &TreasuryCap<T>): u64 {
        cap.total_supply
    }

    /// Give away the treasury cap to `recipient`
    public fun transfer_cap<T>(c: TreasuryCap<T>, recipient: address) {
        Transfer::transfer(c, recipient)
    }

    // === Entrypoints ===

    /// Send `amount` units of `c` to `recipient
    /// Aborts with `EVALUE` if `amount` is greater than or equal to `amount`
    public(script) fun transfer_<T>(c: &mut Coin<T>, amount: u64, recipient: address, ctx: &mut TxContext) {
        Transfer::transfer(withdraw(c, amount, ctx), recipient)
    }

    /// Consume the coin `c` and add its value to `self`.
    /// Aborts if `c.value + self.value > U64_MAX`
    public(script) fun join_<T>(self: &mut Coin<T>, c: Coin<T>, _ctx: &mut TxContext) {
        join(self, c)
    }

    /// Join everything in `coins` with `self`
    public(script) fun join_vec_<T>(self: &mut Coin<T>, coins: vector<Coin<T>>, _ctx: &mut TxContext) {
        join_vec(self, coins)
    }

    /// Split coin `self` to two coins, one with balance `split_amount`,
    /// and the remaining balance is left is `self`.
    public(script) fun split<T>(self: &mut Coin<T>, split_amount: u64, ctx: &mut TxContext) {
        let new_coin = withdraw(self, split_amount, ctx);
        Transfer::transfer(new_coin, TxContext::sender(ctx));
    }

    /// Split coin `self` into multiple coins, each with balance specified
    /// in `split_amounts`. Remaining balance is left in `self`.
    public(script) fun split_vec<T>(self: &mut Coin<T>, split_amounts: vector<u64>, ctx: &mut TxContext) {
        let i = 0;
        let len = Vector::length(&split_amounts);
        while (i < len) {
            split(self, *Vector::borrow(&split_amounts, i), ctx);
            i = i + 1;
        };
    }

    // === Test-only code ===

    #[test_only]
    /// Mint coins of any type for (obviously!) testing purposes only
    public fun mint_for_testing<T>(value: u64, ctx: &mut TxContext): Coin<T> {
        Coin { id: TxContext::new_id(ctx), balance: Balance::create(value) }
    }
}

#[test_only]
module Sui::TestCoin {
    use Sui::TestScenario::{Self, ctx};
    use Sui::Coin;
    use Sui::Balance;
    use Sui::SUI::SUI;

    #[test]
    fun type_morphing() {
        let test = &mut TestScenario::begin(&@0x1);

        let balance = Balance::empty<SUI>();
        let coin = Coin::wrap(balance, ctx(test));
        let balance = Coin::unwrap(coin);

        Balance::destroy_empty(balance);

        let coin = Coin::mint_for_testing<SUI>(100, ctx(test));
        let balance_mut = Coin::balance_mut(&mut coin);
        let sub_balance = Balance::split(balance_mut, 50);

        assert!(Balance::value(&sub_balance) == 50, 0);
        assert!(Coin::value(&coin) == 50, 0);

        let balance = Coin::unwrap(coin);
        Balance::join(&mut balance, sub_balance);

        assert!(Balance::value(&balance) == 100, 0);

        let coin = Coin::wrap(balance, ctx(test));
        Coin::keep(coin, ctx(test));
    }
}
