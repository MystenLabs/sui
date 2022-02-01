module Test::M1 {
    use FastX::Address;
    use FastX::ID::ID;
    use FastX::TxContext::{Self, TxContext};
    use FastX::Transfer;

    struct Object has key, store {
        id: ID,
        value: u64,
    }

    public fun create(value: u64, recipient: vector<u8>, ctx: &mut TxContext) {
        Transfer::transfer(
            Object { id: TxContext::new_id(ctx), value },
            Address::new(recipient)
        )
    }    
}
