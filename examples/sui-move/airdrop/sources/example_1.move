module airdrop::simple {
    use sui::transfer;
    use sui::object::{Self, UID};
    use sui::tx_context::TxContext;

    /// The Object we're distributing to users
    struct Asset has key, store { id: UID }

    public fun mint_to_address(recipient: address, ctx: &mut TxContext) {
        transfer::transfer(Asset {
            id: object::new(ctx)
        }, recipient)
    }
}

