module FastX::Coin {
    use FastX::Address::Address;
    use FastX::ID::ID;
    use FastX::Transfer;
    use FastX::TxContext::{Self, TxContext};
    use Std::Errors;
    use Std::Vector;

    /// A coin of type `T` worth `value`. Transferrable
    struct Coin<phantom T> has key, store {
        id: ID,
        value: u64
    }

    /// Capability allowing the bearer to mint and burn
    /// coins of type `T`. Transferrable
    struct TreasuryCap<phantom T> has key, store {
        id: ID,
        total_supply: u64
    }

    /// Trying to withdraw N from a coin with value < N
    const EVALUE: u64 = 0;
    /// Trying to destroy a coin with a nonzero value
    const ENONZERO: u64 = 0;

    // === Functionality for Coin<T> holders ===

    /// Send `c` to `recipient`
    public fun transfer<T>(c: Coin<T>, recipient: Address) {
        Transfer::transfer(c, recipient)
    }

    /// Consume the coin `c` and add its value to `self`.
    /// Aborts if `c.value + self.value > U64_MAX`
    public fun join<T>(self: &mut Coin<T>, c: Coin<T>) {
        let Coin { id: _, value } = c;
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
        let Coin { id: _, value } = c;
        assert!(value == 0, Errors::invalid_argument(ENONZERO))
    }

    // === Registering new coin types and managing the coin supply ===

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
        value: u64, ctx: &mut TxContext,
    ): Coin<T> {
        //cap.total_supply = cap.total_supply + value;
        Coin { id: TxContext::new_id(ctx), value }
    }

    /// Destroy the coin `c` and decrease the total supply in `cap`
    /// accordingly.
    public fun burn<T>(c: Coin<T>, cap: &mut TreasuryCap<T>) {
        let Coin { id: _, value } = c;
        cap.total_supply = cap.total_supply - value
    }

    /// Return the total number of `T`'s in circulation
    public fun total_supply<T>(cap: &TreasuryCap<T>): u64 {
        cap.total_supply
    }

    /// Give away the treasury cap to `recipient`
    public fun transfer_cap<T>(c: TreasuryCap<T>, recipient: Address) {
        Transfer::transfer(c, recipient)
    }
}
