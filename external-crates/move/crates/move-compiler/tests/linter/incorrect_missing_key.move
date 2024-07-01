module a::m {
    use sui::object::UID;

    struct MissingKeyAbility {
        id: UID,
    }

}

module sui::object {
    struct UID has store {
        id: address,
    }
}
