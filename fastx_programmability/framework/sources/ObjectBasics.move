/// Test CTURD object basics (create, transfer, update, read, delete)
module FastX::ObjectBasics {
    use FastX::Address;
    use FastX::ID::ID;
    use FastX::TxContext::{Self, TxContext};
    use FastX::Transfer;

    struct Object has key {
        id: ID,
        value: u64,
    }

    public fun create(value: u64, recipient: vector<u8>, ctx: &mut TxContext) {
        Transfer::transfer(
            Object { id: TxContext::new_id(ctx), value },
            Address::new(recipient)
        )
    }

    public fun transfer(o: Object, recipient: vector<u8>, _ctx: &mut TxContext) {
        Transfer::transfer(o, Address::new(recipient))
    }

    // test that reading o2 and updating o1 works
    public fun update(o1: &mut Object, o2: &Object, _ctx: &mut TxContext) {
        o1.value = o2.value
    }

    public fun delete(o: Object, _ctx: &mut TxContext) {
        let Object { id: _, value: _ } = o;
    }

}
