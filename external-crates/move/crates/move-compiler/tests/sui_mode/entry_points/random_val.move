// invalid Random by value

module a::m {
    public entry fun no_random_val(_: sui::random::Random) {
        abort 0
    }
}

module sui::random {
    struct Random has key {
        id: sui::object::UID,
    }

    fun test_random_val(_: Random) { abort 0 }
}

module sui::object {
    struct UID has store {
        id: address,
    }
}
