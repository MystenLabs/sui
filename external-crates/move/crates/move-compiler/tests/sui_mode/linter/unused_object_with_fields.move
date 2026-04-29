module a::m {
    use sui::object::{Self, UID};

    const ERR: u64 = 0;

    // An object with no fields.
    struct Empty has key { id: UID }
    // A object/cap with no fields.
    struct AdminCap has key { id: UID }
    // An object/cap with a field that is not used.
    struct OwnerCap has key { id: UID, owns: address }

    // no fields, all fine
    public fun t_1(_e: &Empty, _c: &AdminCap) {}

    // passed by value, all fine
    public fun t_2(_e: &Empty, _c: Empty) { abort ERR }

    // triggered! &OwnerCap has a field
    public fun t_3(_e: &Empty, _c: &OwnerCap) {}

    // triggered! assignment suppression is not enough
    public fun t_4(_e: &Empty, c: &OwnerCap) { let _ = c; }

    // triggered: returning doesn't count
    public fun t_5(_e: &Empty, c: &OwnerCap): &OwnerCap { c }

    // triggered: borrowed value is not used
    public fun t_6(_e: &Empty, c: &OwnerCap) { let _ = c.owns; }

    // not triggered: c is accessed directly
    public fun t_7(_e: &Empty, c: &OwnerCap) { assert!(c.owns == @0, ERR); }

    // not triggered: c is mutated
    public fun t_8(_e: &Empty, c: &mut OwnerCap) { c.owns = @0; }

    // not triggered: c is passed to internal_check
    public fun t_9(_e: &Empty, c: &OwnerCap) { internal_check(c); }

    // not triggered: field is passed to a function
    public fun t_10(_e: &Empty, c: &OwnerCap) { assert_owner(c.owns) }

    // not triggered: passed by value
    public fun t_11(_e: &Empty, c: OwnerCap) { consume(c) }

    // not triggered: passed by value (testing with unpack)
    public fun t_12(_e: &Empty, c: OwnerCap) {
        let OwnerCap { id, owns: _ } = c;
        object::delete(id)
    }

    // === Other ===

    // typical getter function, should not be affected
    public fun owns(c: &OwnerCap): address { c.owns }

    // typical pattern for vectors / vec_sets: should not be triggered
    public fun owns_mut(c: &mut OwnerCap): &mut address { &mut c.owns }

    // typical setter function, should not be affected
    public fun set_owns(c: &mut OwnerCap, owns: address) { c.owns = owns; }

    fun internal_check(c: &OwnerCap) {
        assert!(c.owns == @0, ERR);
    }

    fun assert_owner(c: address) {
        assert!(c == @0, ERR);
    }

    fun consume<T>(_: T) { abort ERR }
}

module sui::object {
    use sui::tx_context::TxContext;

    const ZERO: u64 = 0;

    struct UID has store, drop {
        id: address,
    }

    public fun new(_: &mut TxContext): UID {
        abort ZERO
    }

    public fun delete(u: UID) {
        let UID { id: _ } = u;
    }
}

module sui::tx_context {
    struct TxContext has drop {}

    public fun sender(_: &TxContext): address {
        @0
    }
}
