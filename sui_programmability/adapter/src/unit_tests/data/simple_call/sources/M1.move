module Test::M1 {
    use Sui::ID::VersionedID;
    use Sui::TxContext::{Self, TxContext};
    use Sui::Transfer;
    use Sui::Coin::Coin;

    struct Object has key, store {
        id: VersionedID,
        value: u64,
    }

    fun foo<T: key, T2: drop>(_p1: u64, value1: T, _value2: &Coin<T2>, _p2: u64): T {
        value1 
    }

    public fun create(value: u64, recipient: address, ctx: &mut TxContext) {
        Transfer::transfer(
            Object { id: TxContext::new_id(ctx), value },
            recipient
        )
    }    
}
