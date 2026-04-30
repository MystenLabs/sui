// Cases where the lint correctly warns.

module a::m {
    use sui::object::UID;

    struct OwnerCap has key { id: UID, owns: address }
    struct ValueCap has key { id: UID, value: u64 }
    struct Wrapper has drop { val: u64, other: u64 }

    // not touched at all
    public fun unused(_c: &OwnerCap) {}

    // assigned to `_` — assignment alone does not count as use
    public fun let_underscore(c: &OwnerCap) { let _ = c; }

    // returned without ever accessing a field
    public fun returned_as_root(c: &OwnerCap): &OwnerCap { c }

    // a field is borrowed but the result is discarded
    public fun field_discarded(c: &OwnerCap) { let _ = c.owns; }

    // a field is cast but the result is discarded
    public fun cast_discarded(c: &ValueCap) { let _ = (c.value as u128); }

    // a binop result is discarded
    public fun binop_discarded(c: &ValueCap) { let _ = c.value + 1; }

    // Pack carries per-field tracking. `c.value` lands in `val`, but only the
    // unrelated `other` field is read — the lint can tell `c` was never
    // actually utilised downstream.
    public fun pack_other_field(c: &ValueCap) {
        let w = Wrapper { val: c.value, other: 0 };
        let _ = w.other;
    }

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
