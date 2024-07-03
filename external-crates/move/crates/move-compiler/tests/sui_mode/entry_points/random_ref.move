// valid Random by immutable reference

module a::m {
    public entry fun yes_random_ref(_: &sui::random::Random) {
        abort 0
    }
}

module sui::random {
    struct Random has key {
        id: sui::object::UID,
    }
}

module sui::object {
    struct UID has store {
        id: address,
    }
}
