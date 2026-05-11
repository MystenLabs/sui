// Cases where the lint correctly stays silent.

#[allow(lint(unnecessary_unit, abort_without_constant))]
module a::m {
    use sui::object::{Self, UID};

    const ERR: u64 = 0;

    struct AdminCap has key { id: UID }
    struct OwnerCap has key { id: UID, owns: address }
    struct ValueCap has key { id: UID, value: u64 }
    struct BoolCap has key { id: UID, flag: bool }
    struct ColorObject has key { id: UID, red: u8, green: u8, blue: u8 }
    struct Wrapper has drop { val: u64, other: u64 }
    struct Wrapped has drop { f: bool }

    // no fields beyond `id` — does not qualify
    public fun no_fields(_c: &AdminCap) {}

    // by-value parameters are out of scope
    public fun by_value(c: OwnerCap) { consume(c) }
    public fun by_value_unpacked(c: OwnerCap) {
        let OwnerCap { id, owns: _ } = c;
        object::delete(id)
    }

    // field read in a binop whose result is then consumed
    public fun field_read_in_binop(c: &OwnerCap) { assert!(c.owns == @0, ERR); }

    // field used as a JumpIf condition (incl. bool field directly)
    public fun cond_bool_field(c: &BoolCap) { if (c.flag) {} else {} }
    public fun cond_field_compare(c: &ValueCap) { if (c.value > 0) {} else {} }

    // field written
    public fun field_written(c: &mut OwnerCap) { c.owns = @0; }

    // field read on the RHS of a field write into a `&mut` input — the
    // value flows out of the function via the mutated input.
    public fun copy_into(from: &ColorObject, into: &mut ColorObject) {
        into.red = from.red;
        into.green = from.green;
        into.blue = from.blue;
    }

    // entire object passed to another function — including the common
    // `object::id(c)`-style pattern where the whole `&c` is forwarded
    public fun passed_to_fn(c: &OwnerCap) { check(c); }
    public fun id_of(c: &OwnerCap): address { object_id(c) }

    // a field passed to another function
    public fun field_passed_to_fn(c: &OwnerCap) { assert_addr(c.owns); }

    // both branches contribute to a single local — neither var should be
    // flagged when the joined value is consumed
    public fun branch_join(c1: &OwnerCap, c2: &OwnerCap, cond: bool) {
        let tmp = if (cond) c1 else c2;
        check(tmp);
    }

    // typical accessor patterns
    public fun get(c: &OwnerCap): address { c.owns }
    public fun get_mut(c: &mut OwnerCap): &mut address { &mut c.owns }
    public fun set(c: &mut OwnerCap, owns: address) { c.owns = owns; }

    // field cast then consumed downstream
    public fun cast_then_returned(c: &ValueCap): u128 { (c.value as u128) }
    public fun cast_then_compared(c: &ValueCap) { assert!((c.value as u128) > 0, ERR); }

    // binop result consumed downstream
    public fun binop_then_returned(c: &ValueCap): u64 { c.value + 1 }
    public fun binop_then_compared(c: &ValueCap) { assert!(c.value + 1 > 0, ERR); }

    // Pack carries per-field tracking. Returning the packed struct exposes
    // its tracked fields to the caller and counts as use.
    public fun pack_returned(c: &ValueCap): Wrapper {
        Wrapper { val: c.value, other: 0 }
    }

    // Reading the field that was packed with `c.value` recovers tracking.
    public fun pack_then_read_tracked(c: &ValueCap): u64 {
        let w = Wrapper { val: c.value, other: 0 };
        w.val
    }

    // Passing the packed struct to a function counts as use.
    public fun pack_then_pass(c: &ValueCap) {
        let w = Wrapper { val: c.value, other: 0 };
        consume_wrapper(w);
    }

    // binop result stored in a local, then asserted on
    public fun binop_local_then_assert(o: &ValueCap) {
        let b = o.value + 10;
        assert!(b == 20, ERR);
    }

    // binop result stored in a local, then returned
    public fun binop_local_then_return(o: &ValueCap): u64 {
        let b = o.value + 10;
        b
    }

    // packed into a local struct, tracked field then read in an assertion
    public fun pack_local_then_assert_field(o: &BoolCap) {
        let t = Wrapped { f: o.flag };
        assert!(t.f, ERR);
    }

    // doubly-negated binop used as an assertion condition
    public fun double_negated_binop_in_assert(o: &ValueCap) {
        assert!(!(o.value > 10), ERR);
    }

    // ref-pattern destructure with a binding that is later used
    public fun unpack_ref_then_use_binding(o: &ValueCap) {
        let ValueCap { id: _, value } = o;
        assert!(*value == 10, ERR);
    }

    // Reading a field counts as a use even if the result is dropped —
    // touching the field is what we care about.
    public fun field_discarded(c: &OwnerCap) { let _ = c.owns; }
    public fun cast_discarded(c: &ValueCap) { let _ = (c.value as u128); }
    public fun binop_discarded(c: &ValueCap) { let _ = c.value + 1; }

    // Pack of a tracked field whose result is dropped — we still read
    // the field to put it in the wrapper.
    public fun pack_other_field(c: &ValueCap) {
        let w = Wrapper { val: c.value, other: 0 };
        let _ = w.other;
    }

    // `abort` of a field-derived value: reading the field still counts.
    public fun abort_with_field(c: &ValueCap, flag: bool) {
        if (flag) abort c.value
    }

    // Vector literal of a field-derived value (returned).
    public fun vector_returned(c: &ValueCap): vector<u64> {
        vector[c.value]
    }

    fun check(c: &OwnerCap) { assert!(c.owns == @0, ERR); }
    fun assert_addr(a: address) { assert!(a == @0, ERR); }
    fun consume_wrapper(_w: Wrapper) { abort ERR }

    #[allow(lint(unused_object_with_fields))]
    fun object_id(_c: &OwnerCap): address { @0 }

    fun consume<T>(_: T) { abort ERR }
}

module sui::object {
    struct UID has store, drop { id: address }
    public fun delete(u: UID) { let UID { id: _ } = u; }
}
