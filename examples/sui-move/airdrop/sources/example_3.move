module airdrop::simple_v3 {
    use std::vector;
    use sui::transfer;
    use sui::object::{Self, UID};
    use sui::tx_context::TxContext;

    /// The Object we're distributing to users
    struct Asset has key, store { id: UID }

    /// Transferred to publisher during module init (omitting this part)
    struct AdminCap has key, store { id: UID }

    /// Now to multiple recipients
    public fun mint_to_addresses(
        _: &AdminCap, recipients: vector<address>, ctx: &mut TxContext
    ) {
        let (i, len) = (0, vector::length(&recipients));
        while (i < len) {
            let recipient = vector::pop_back(&mut recipients);

            transfer::transfer(Asset {
                id: object::new(ctx)
            }, recipient);
            i = i + 1;
        }
    }
}

// #[test] fun test() {
//     let ctx = &mut sui::tx_context::dummy();
//     let cap = AdminCap { id: object::new(ctx) };

//     mint_to_addresses(&cap, vector[@0x2, @0x3, @0x4], ctx);

//     let AdminCap { id } = cap;
//     object::delete(id);
// }
