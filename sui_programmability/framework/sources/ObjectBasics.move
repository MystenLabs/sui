/// Test CTURD object basics (create, transfer, update, read, delete)
module Sui::ObjectBasics {
    use Sui::Event;
    use Sui::ID::{Self, VersionedID};
    use Sui::TxContext::{Self, TxContext};
    use Sui::Transfer;

    struct Object has key, store {
        id: VersionedID,
        value: u64,
    }

    struct Wrapper has key {
        id: VersionedID,
        o: Object 
    }

    struct NewValueEvent has copy, drop {
        new_value: u64
    }

    public fun create(value: u64, recipient: address, ctx: &mut TxContext) {
        Transfer::transfer(
            Object { id: TxContext::new_id(ctx), value },
            recipient
        )
    }

    public fun transfer(o: Object, recipient: address, _ctx: &mut TxContext) {
        Transfer::transfer(o, recipient)
    }

    public fun freeze_object(o: Object, _ctx: &mut TxContext) {
        Transfer::freeze_object(o)
    }

    public fun set_value(o: &mut Object, value: u64, _ctx: &mut TxContext) {
        o.value = value;
    }

    // test that reading o2 and updating o1 works
    public fun update(o1: &mut Object, o2: &Object, _ctx: &mut TxContext) {
        o1.value = o2.value;
        // emit an event so the world can see the new value
        Event::emit(NewValueEvent { new_value: o2.value })
    }

    public fun delete(o: Object, _ctx: &mut TxContext) {
        let Object { id, value: _ } = o;
        ID::delete(id);
    }

    public fun wrap(o: Object, ctx: &mut TxContext) {
        Transfer::transfer(Wrapper { id: TxContext::new_id(ctx), o }, TxContext::sender(ctx))
    }

    public fun unwrap(w: Wrapper, ctx: &mut TxContext) {
        let Wrapper { id, o } = w;
        ID::delete(id);
        Transfer::transfer(o, TxContext::sender(ctx))
    }
}
