// valid, Receiving type by mut ref with object type param

module a::m {
    use sui::object;
    use sui::transfer::Receiving;

    struct S has key { id: object::UID }

    public entry fun yes(_: &mut Receiving<S>) { }
}

module sui::object {
    struct UID has store {
        id: address,
    }
}

module sui::transfer {
    struct Receiving<phantom T: key> has drop {
        id: address
    }
}
