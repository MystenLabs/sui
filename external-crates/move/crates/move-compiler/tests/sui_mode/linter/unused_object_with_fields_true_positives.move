// Cases where the lint correctly warns.

module a::m {
    use sui::object::UID;

    struct OwnerCap has key { id: UID, owns: address }
    struct ValueCap has key { id: UID, value: u64 }

    struct Inner has key, store { id: UID, value: u64 }
    struct Outer has key { id: UID, inner: Inner }

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

    // Both branches contribute a bare-ref pass-through to the return —
    // returning a bare reference does not count as use of any field,
    // so both roots remain unused.
    public fun join_then_return(c1: &OwnerCap, c2: &OwnerCap, cond: bool): &OwnerCap {
        if (cond) c1 else c2
    }

    // The joined value carries `a` as a bare ref and `outer` as field-derived.
    // Because per-root tracking distinguishes the two kinds, only `outer` is
    // counted as used at the return; `a` remains a pass-through and is flagged.
    public fun mixed_branch_return(
        a: &Inner,
        outer: &Outer,
        cond: bool
    ): &Inner {
        if (cond) a else &outer.inner
    }
}

module sui::object {
    struct UID has store, drop { id: address }
}
