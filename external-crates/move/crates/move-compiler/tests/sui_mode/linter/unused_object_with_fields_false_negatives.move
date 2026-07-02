// Cases where the lint stays silent even though the object may be unused.
// These are known blind spots.

module a::m {
    use sui::object::UID;

    struct OwnerCap has key { id: UID, owns: address }
    struct GenericCap<phantom T> has key { id: UID, owns: address }

    // Generic object types are out of scope for this lint.
    public fun generic_unused<T>(_c: &GenericCap<T>) {}

    // Calls are treated as usage, but a generic callee's `&T` parameter is
    // outside the lint's object-shape checks.
    public fun forwarded_to_generic_no_op(c: &OwnerCap) {
        generic_no_op(c);
    }

    fun generic_no_op<T>(_: &T) {}
}

module sui::object {
    struct UID has store, drop { id: address }
}
