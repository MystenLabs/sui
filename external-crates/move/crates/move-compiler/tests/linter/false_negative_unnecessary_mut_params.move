module 0x42::false_positives {
    use sui::object::UID;

    struct Person has key, store {
        id: UID,
        count: u64
    }

    // Case 1: Indirect mutation through multiple layers
    public fun indirect_mutation(person: &mut Person) {
        nested_helper_one(person);
    }

    fun nested_helper_one(person: &mut Person) {
        nested_helper_two(person);
    }

    fun nested_helper_two(person: &mut Person) {
        person.count = person.count + 1;
    }

    // Case 2: Generic mutation
    public fun generic_mutation<T: store>(value: &mut T) {
        // Complex generic operations that might require mutability
    }

    // Case 3: Conditional complex modification
    public fun dynamic_mutation(person: &mut Person, should_modify: bool) {
        if (should_modify) {
            nested_helper_one(person);
        };
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
