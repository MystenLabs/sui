// Cases where the lint correctly warns.

module a::m {
    use sui::object::UID;

    struct OwnerCap has key { id: UID, owns: address }
    struct ValueCap has key { id: UID, value: u64 }

    // not touched at all
    public fun unused(_c: &OwnerCap) {}

    // assigned to `_` — assignment alone does not count as use
    public fun let_underscore(c: &OwnerCap) { let _ = c; }

    // returned without ever accessing a field
    public fun returned_as_root(c: &OwnerCap): &OwnerCap { c }

    // ref-pattern destructure that ignores every field
    public fun unpack_all_ignored(o: &ValueCap) {
        let ValueCap { id: _, value: _ } = o;
    }

    // ref-pattern destructure that binds a field but never consumes it
    public fun unpack_bound_unused(o: &ValueCap) {
        let ValueCap { id: _, value: _v } = o;
    }
}

module sui::object {
    struct UID has store, drop { id: address }
}
