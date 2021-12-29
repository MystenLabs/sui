/// Test CTURD object basics (create, transfer, update, read, delete)
module FastX::ObjectBasics {
    use FastX::Authenticator;
    use FastX::ID::ID;
    use FastX::TxContext::{Self, TxContext};
    use FastX::Transfer;

    struct Object has key {
        id: ID,
        value: u64,
    }

    public fun create(value: u64, recipient: vector<u8>, ctx: TxContext) {
        Transfer::transfer(Object { id: TxContext::new_id(&mut ctx), value }, Authenticator::new(recipient))
    }

    public fun transfer(o: Object, recipient: vector<u8>, _ctx: TxContext) {
        Transfer::transfer(o, Authenticator::new(recipient))
    }

    // test that reading o2 and updating o1 works
    public fun update(o1: &mut Object, o2: &Object, _ctx: TxContext) {
        o1.value = o2.value
    }

    public fun delete(o: Object, _ctx: TxContext) {
        let Object { id: _, value: _ } = o;
    }

}
