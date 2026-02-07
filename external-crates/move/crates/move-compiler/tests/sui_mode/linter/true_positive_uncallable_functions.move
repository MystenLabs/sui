// invalid cases. Clock and Random must be passed by immutable reference.
// TxContext can appear multiple times but uniquely if mutable. And cannot be owned.

module a::m {
    const ERR: u64 = 0;
    fun no_clock_mut(_: &mut sui::clock::Clock) {
        abort ERR
    }
    fun no_clock_val(_: sui::clock::Clock) {
        abort ERR
    }
    fun no_random_mut(_: &mut sui::random::Random) {
        abort ERR
    }
    fun no_random_val(_: sui::random::Random) {
        abort ERR
    }

    use sui::tx_context::TxContext;
    fun two_mut_ctx(_: &mut TxContext, _: &mut TxContext) {
        abort ERR
    }
    fun mut_and_imm_ctx(_: &mut TxContext, _: &TxContext) {
        abort ERR
    }
    fun owned_ctx(_: TxContext, _: &mut TxContext, _: &mut TxContext) {
        abort ERR
    }

}

module sui::clock {
    const ERR: u64 = 0;

    struct Clock has key {
        id: sui::object::UID,
    }

    // no warning
    fun test_clock_mut(_: &mut Clock) {}
    // no warning
    fun test_clock_val(_: Clock) { abort ERR }
}

module sui::random {
    const ERR: u64 = 0;

    struct Random has key {
        id: sui::object::UID,
    }

    // no warning
    fun test_random_mut(_: &mut Random) {}
    // no warning
    fun test_random_val(_: Random) { abort ERR }
}


module sui::object {
    struct UID has store {
        id: address,
    }
}

module sui::tx_context {
    struct TxContext has drop {}
}
