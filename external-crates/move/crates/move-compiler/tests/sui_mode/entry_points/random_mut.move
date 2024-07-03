// invalid Random by mutable reference

module a::m {
    public entry fun no_random_mut(_: &mut sui::random::Random) {
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
