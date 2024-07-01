module 0x42::M {
    use sui::object::UID;
    struct Person has key, store {
        id: UID,
        count: u64,
    }

    public fun modify_param(person: &mut Person) {
        person.count = person.count + 1;
    }

    public fun pass_modify_param(person: &mut Person, x: u64) {
        actually_modify_param(person, x);
    }

    // Helper function that modifies a mutable reference
    public fun actually_modify_param(x: &mut Person, y: u64) {
        x.count = x.count + y;
    }
}

module sui::object {
    struct UID has store {
        id: address,
    }
}
