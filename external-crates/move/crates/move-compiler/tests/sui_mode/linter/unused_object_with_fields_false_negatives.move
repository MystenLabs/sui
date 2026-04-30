// Cases where the lint stays silent even though the object is, in practice,
// not really used. Limitations of the local analysis.

module a::m {
    use sui::object::UID;

    struct OwnerCap has key { id: UID, owns: address }

    // forwarding the object to a function that ignores it is treated as
    // use, since the lint does not look inside the callee
    public fun forwarded_to_no_op(c: &OwnerCap) {
        no_op(c);
    }

    #[allow(lint(unused_object_with_fields))]
    fun no_op(_c: &OwnerCap) {}
}

module sui::object {
    struct UID has store, drop { id: address }
}
