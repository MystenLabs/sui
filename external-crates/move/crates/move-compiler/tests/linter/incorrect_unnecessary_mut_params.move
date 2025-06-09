module 0x42::M {
    use sui::object::UID;
    struct Person has key, store {
        id: UID,
        count: u64,
    }

    public fun not_modify_param(person: &mut Person) {
        person;
    }
}

module sui::object {
    struct UID has store {
        id: address,
    }
}
