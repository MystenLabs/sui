module Test::M1 {
    use Sui::ID::VersionedID;
    use Sui::TxContext::{Self, TxContext};
    use Sui::Transfer;

    struct Object has key, store {
        id: VersionedID,
        value: u64,
    }

    // initializer that should be executed upon publishing this module
    fun init(ctx: &mut TxContext): u64 {
        let value = 42;
        let singleton = Object { id: TxContext::new_id(ctx), value };
        Transfer::transfer(singleton, TxContext::sender(ctx));
        value
    }
}
