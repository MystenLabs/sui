module Test::M1 {
    use Sui::ID::VersionedID;
    use Sui::TxContext::{Self, TxContext};
    use Sui::Transfer;

    struct Object has key, store {
        id: VersionedID,
        value: u64,
    }
    const ADMIN: address = @0xa5e6dbcf33730ace6ec8b400ff4788c1f150ff7e;

    // initializer that should be executed upon publishing this module
    fun init(ctx: &mut TxContext) {
        let singleton = Object { id: TxContext::new_id(ctx), value: 12 };
        Transfer::transfer(singleton, ADMIN)
    }
}
