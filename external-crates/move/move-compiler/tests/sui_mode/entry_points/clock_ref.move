// valid, Clock by immutable reference

module a::m {
    public entry fun yes_clock_ref(_: &sui::clock::Clock) {
        abort 0
    }
}

module sui::clock {
    struct Clock has key {
        id: sui::object::UID,
    }
}

module sui::object {
    struct UID has store {
        id: address,
    }
}
