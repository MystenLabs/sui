module airdrop::simple_v4 {
    use std::vector;
    use sui::transfer;
    use sui::object::{Self, UID};
    use sui::tx_context::{sender, TxContext};

    const EHashesAlreadySet: u64 = 0;
    const EHashNotFound: u64 = 1;

    /// The Object we're distributing to users.
    struct Asset has key, store { id: UID }

    /// One-time use capability to setup hashes.
    struct SetupCap has key, store { id: UID }

    /// The setup object, contains the hashes of the recipients
    struct Setup has key { id: UID, hashes: vector<vector<u8>> }

    fun init(ctx: &mut TxContext) {
        transfer::transfer(SetupCap { id: object::new(ctx) }, sender(ctx));
        transfer::share_object(Setup {
            id: object::new(ctx),
            hashes: vector[]
        })
    }

    /// One time use function to set the hashes.
    entry fun prepare(
        cap: SetupCap,
        setup: &mut Setup,
        hashes: vector<vector<u8>>
    ) {
        assert!(vector::length(&hashes) == 0, EHashesAlreadySet);
        let SetupCap { id } = cap;
        object::delete(id);

        setup.hashes = hashes;
    }
}




// #[test] fun test() {
//     let ctx = &mut sui::tx_context::dummy();
//     let cap = AdminCap { id: object::new(ctx) };

//     mint_to_addresses(&cap, vector[@0x2, @0x3, @0x4], ctx);

//     let AdminCap { id } = cap;
//     object::delete(id);
// }
