// Cases where the lint stays silent even though the object may be unused.
// These are known blind spots.

module a::m {
    use sui::object::UID;

    struct GenericCap<phantom T> has key { id: UID, owns: address }

    // Generic object types are out of scope for this lint.
    public fun generic_unused<T>(_c: &GenericCap<T>) {}
}

module sui::object {
    struct UID has store, drop { id: address }
}
