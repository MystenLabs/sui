// valid
module a::m {
    use sui::object;
    struct S has key {
        id: object::UID
    }
}

module sui::object {
    struct UID has store {
        id: address,
    }
}
