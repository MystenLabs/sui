module airdrop::simple_v2 {
    use sui::transfer;
    use sui::object::{Self, UID};
    use sui::tx_context::TxContext;

    /// The Object we're distributing to users
    struct Asset has key, store { id: UID }

    /// Transferred to publisher during module init (omitting this part)
    struct AdminCap has key, store { id: UID }

    public fun mint_to_address(
        _: &AdminCap, recipient: address, ctx: &mut TxContext
    ) {
        transfer::transfer(Asset {
            id: object::new(ctx)
        }, recipient)
    }
}

