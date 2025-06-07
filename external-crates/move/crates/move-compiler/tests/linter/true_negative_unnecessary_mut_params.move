module 0x42::true_negatives {
    use sui::object::UID;

    struct Person has key, store {
        id: UID,
        count: u64,
        name: vector<u8>
    }

    // Case 1: Direct modification
    public fun modify_person(person: &mut Person) {
        person.count = person.count + 1;
    }

    // Case 2: Indirect modification through helper
    public fun pass_modify_person(person: &mut Person, x: u64) {
        actually_modify_person(person, x);
    }

    // Helper function
    public fun actually_modify_person(person: &mut Person, value: u64) {
        person.count = person.count + value;
    }

    // Case 3: Multiple modifications
    public fun multiple_modifications(person: &mut Person) {
        person.count = person.count + 1;
        person.name = b"new name";
    }
}

module sui::object {
    struct UID has store {
        id: address
    }

    public fun new(id: address): UID {
        UID { id }
    }
}
