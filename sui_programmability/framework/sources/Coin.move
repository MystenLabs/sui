module Sui::Coin {
    use Sui::ID::{Self, VersionedID};
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};
    use Std::Errors;
    use Std::Vector;

    /// A coin of type `T` worth `value`. Transferrable
    struct Coin<phantom T> has key, store {
        id: VersionedID,
        value: u64
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
        let Coin { id, value } = c;
        ID::delete(id);
        self.value = self.value + value
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
        assert!(
            self.value >= value,
            Errors::limit_exceeded(EVALUE)
        );
        self.value = self.value - value;
        Coin { id: TxContext::new_id(ctx), value }
    }

    /// Public getter for the coin's value
    public fun value<T>(self: &Coin<T>): u64 {
        self.value
    }

    /// Destroy a coin with value zero
    public fun destroy_zero<T>(c: Coin<T>) {
        let Coin { id, value } = c;
        ID::delete(id);
        assert!(value == 0, Errors::invalid_argument(ENONZERO))
    }

    // === Registering new coin types and managing the coin supply ===

    /// Make any Coin with a zero value. Useful for placeholding
    /// bids/payments or preemptively making empty balances.
    public fun zero<T>(ctx: &mut TxContext): Coin<T> {
        Coin { id: TxContext::new_id(ctx), value: 0 }
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
        Coin { id: TxContext::new_id(ctx), value }
    }

    /// Destroy the coin `c` and decrease the total supply in `cap`
    /// accordingly.
    public fun burn<T>(c: Coin<T>, cap: &mut TreasuryCap<T>) {
        let Coin { id, value } = c;
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
    public fun transfer_<T>(c: &mut Coin<T>, amount: u64, recipient: address, ctx: &mut TxContext) {
        Transfer::transfer(withdraw(c, amount, ctx), recipient)
    }

    /// Consume the coin `c` and add its value to `self`.
    /// Aborts if `c.value + self.value > U64_MAX`
    public fun join_<T>(self: &mut Coin<T>, c: Coin<T>, _ctx: &mut TxContext) {
        join(self, c)
    }

    /// Join everything in `coins` with `self`
    public fun join_vec_<T>(self: &mut Coin<T>, coins: vector<Coin<T>>, _ctx: &mut TxContext) {
        join_vec(self, coins)
    }

    /// Split coin `self` to two coins, one with balance `split_amount`,
    /// and the remaining balance is left is `self`.
    public fun split<T>(self: &mut Coin<T>, split_amount: u64, ctx: &mut TxContext) {
        let new_coin = withdraw(self, split_amount, ctx);
        Transfer::transfer(new_coin, TxContext::sender(ctx));
    }

    /// Split coin `self` into multiple coins, each with balance specified
    /// in `split_amounts`. Remaining balance is left in `self`.
    public fun split_vec<T>(self: &mut Coin<T>, split_amounts: vector<u64>, ctx: &mut TxContext) {
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
        Coin { id: TxContext::new_id(ctx), value }
    }
}
