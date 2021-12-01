/// Example coin with a trusted owner responsible for minting/burning (e.g., a stablecoin)
module Examples::TrustedCoin {
    use FastX::Coin;
    use FastX::Transfer;
    use FastX::TxContext::{Self, TxContext};

    /// Name of the coin
    struct EXAMPLE has drop {}

    /// Register the trusted currency to acquire its `TreasuryCap`. Because
    /// this is a module initializer, it ensures the currency only gets
    /// registered once.
    // TODO: this uses a module initializer, which doesn't exist in Move.
    // However, we can (and I think should) choose to support this in the FastX
    // adapter to enable us -cases that require at-most-once semantics
    fun init(ctx: &mut TxContext) {
        // Get a treasury cap for the coin and give it to the transaction
        // sender
        let treasury_cap = Coin::create_currency<EXAMPLE>(EXAMPLE{}, ctx);
        Transfer::transfer(treasury_cap, TxContext::get_authenticator(ctx))
    }
}
