/// Example coin with a trusted owner responsible for minting/burning (e.g., a stablecoin)
module Examples::TrustedCoin {
    use Sui::Coin::{Self, TreasuryCap};
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};

    /// Name of the coin
    struct EXAMPLE has drop {}

    /// Register the trusted currency to acquire its `TreasuryCap`. Because
    /// this is a module initializer, it ensures the currency only gets
    /// registered once.
    fun init(ctx: &mut TxContext) {
        // Get a treasury cap for the coin and give it to the transaction
        // sender
        let treasury_cap = Coin::create_currency<EXAMPLE>(EXAMPLE{}, ctx);
        Transfer::transfer(treasury_cap, TxContext::sender(ctx))
    }

    public fun mint(treasury_cap: &mut TreasuryCap<EXAMPLE>, amount: u64, ctx: &mut TxContext) {
        let coin = Coin::mint<EXAMPLE>(amount, treasury_cap, ctx);
        Coin::transfer(coin, TxContext::sender(ctx));
    }

    public fun transfer(treasury_cap: TreasuryCap<EXAMPLE>, recipient: address, _ctx: &mut TxContext) {
        Coin::transfer_cap<EXAMPLE>(treasury_cap, recipient);
    }

    #[test_only]
    /// Wrapper of module initializer for testing
    public fun test_init(ctx: &mut TxContext) {
        init(ctx)
    }
}
