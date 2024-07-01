module a::m {
    use sui::object::UID;

    struct WithKeyAbility has key {
        id: UID,
    }

}

module sui::object {
    struct UID has store {
        id: address,
    }
}
