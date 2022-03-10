/// Example coin with a trusted manager responsible for minting/burning (e.g., a stablecoin)
/// By convention, modules defining custom coin types use upper case names, in constrast to
/// ordinary modules, which use camel case.
module FungibleTokens::MANAGED {
    use Sui::Coin::{Self, Coin, TreasuryCap};
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};

    /// Name of the coin. By convention, this type has the same name as its parent module
    /// and has no fields. The full type of the coin defined by this module will be `COIN<MANAGED>`.
    struct MANAGED has drop {}

    /// Register the trusted currency to acquire its `TreasuryCap`. Because
    /// this is a module initializer, it ensures the currency only gets
    /// registered once.
    fun init(ctx: &mut TxContext) {
        // Get a treasury cap for the coin and give it to the transaction
        // sender
        let treasury_cap = Coin::create_currency<MANAGED>(MANAGED{}, ctx);
        Transfer::transfer(treasury_cap, TxContext::sender(ctx))
    }

    /// Manager can mint new coins
    public fun mint(treasury_cap: &mut TreasuryCap<MANAGED>, amount: u64, ctx: &mut TxContext): Coin<MANAGED> {
        Coin::mint<MANAGED>(amount, treasury_cap, ctx)
    }

    /// Manager can burn coins
    public fun burn(treasury_cap: &mut TreasuryCap<MANAGED>, coin: Coin<MANAGED>, _ctx: &mut TxContext) {
        Coin::burn(coin, treasury_cap)
    }

    /// Manager can transfer the treasury capability to a new manager
    public fun transfer_cap(treasury_cap: TreasuryCap<MANAGED>, recipient: address, _ctx: &mut TxContext) {
        Coin::transfer_cap<MANAGED>(treasury_cap, recipient);
    }

    #[test_only]
    /// Wrapper of module initializer for testing
    public fun test_init(ctx: &mut TxContext) {
        init(ctx)
    }
}
