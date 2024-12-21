module 0x42::true_positives {
    use sui::object::UID;
    
    struct Person has key, store {
        id: UID,
        count: u64,
        name: vector<u8>,
        active: bool
    }

    struct Counter has key, store {
        id: UID,
        value: u64
    }

    // Case 1: Mutable parameter not modified directly or indirectly
    public fun unused_mut_person(person: &mut Person) {
        let _count = person.count;  // Only reading
    }

    // Case 2: Multiple unused mutable parameters
    public fun multiple_unused_muts(person: &mut Person, counter: &mut Counter) {
        let _sum = person.count + counter.value;  // Only reading both
    }

    // Case 3: One used, one unused mutable parameter
    public fun partial_unused_muts(person: &mut Person, counter: &mut Counter) {
        counter.value = counter.value + 1;  // Only counter is modified
        let _count = person.count;  // person is only read
    }

    // Case 4: Unused mutable in complex logic
    public fun complex_unused_mut(person: &mut Person, immut_value: u64) {
        let _result = if (person.count > immut_value) {
            person.count  // Only reading
        } else {
            immut_value
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
