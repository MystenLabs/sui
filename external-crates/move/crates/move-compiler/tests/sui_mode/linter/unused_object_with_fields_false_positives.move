// Cases where the lint warns even though the object is, in some sense, used.
// These are limitations of the local analysis: a handful of operations drop
// or don't follow the tracked value before any consumer can mark it.
//
// (No known false positives currently — placeholder kept for future cases.)

module a::m {
    use sui::object::UID;

    struct ValueCap has key { id: UID, value: u64 }

    // Touch ValueCap to keep the module valid.
    public fun touch(c: &ValueCap): u64 { c.value }
}

module sui::object {
    struct UID has store, drop { id: address }
}
