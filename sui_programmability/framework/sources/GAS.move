/// Coin<Gas> is the token used to pay for gas in Sui
module Sui::GAS {
    use Sui::Coin;
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};

    /// Name of the coin
    struct GAS has drop {}

    /// Register the token to acquire its `TreasuryCap`. Because
    /// this is a module initializer, it ensures the currency only gets
    /// registered once.
    // TODO(https://github.com/MystenLabs/sui/issues/90): implement module initializers
    fun init(ctx: &mut TxContext) {
        // Get a treasury cap for the coin and give it to the transaction sender
        let treasury_cap = Coin::create_currency(GAS{}, ctx);
        Transfer::transfer(treasury_cap, TxContext::sender(ctx))
    }

    /// Transfer to a recipient
    public fun transfer(c: Coin::Coin<GAS>, recipient: address, _ctx: &mut TxContext) {
        Coin::transfer(c, recipient)
    }

}
