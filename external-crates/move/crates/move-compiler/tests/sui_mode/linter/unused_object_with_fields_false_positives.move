// Cases where the lint warns even though the object is, in some sense, used.
// These are limitations of the local analysis: a handful of operations drop
// or don't follow the tracked value before any consumer can mark it.

#[allow(lint(abort_without_constant))]
module a::m {
    use sui::object::UID;

    struct ValueCap has key { id: UID, value: u64 }

    // RHS of a mutate is evaluated for side effects only — the field value
    // flows into a place the lint cannot follow.
    public fun mutate_rhs(other: &mut ValueCap, c: &ValueCap) {
        other.value = c.value;
    }

    // `abort` payload is not counted as use.
    public fun abort_with_field(c: &ValueCap, flag: bool) {
        if (flag) abort c.value
    }

    // Vector literals drop tracking; the framework returns n values for an
    // n-element literal which makes per-index propagation awkward.
    public fun vector_returned(c: &ValueCap): vector<u64> {
        vector[c.value]
    }
}

module sui::object {
    struct UID has store, drop { id: address }
}
