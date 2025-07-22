module 0x42::false_negatives {
    use sui::object::UID;

    struct Person has key, store {
        id: UID,
        count: u64
    }

    // Case 1: Mutation in dead code
    public fun dead_code_mutation(person: &mut Person): u64 {
        return person.count;
    }

    // Case 2: Conditional mutation that never executes
    public fun conditional_mutation(person: &mut Person) {
        if (false) {
            person.count = person.count + 1;
        };
    }

    // Case 3: Complex control flow
    public fun complex_flow_mutation(person: &mut Person, flag: bool) {
        if (flag && false) {
            person.count = 0;
        } else if (!flag && false) {
            person.count = 1;
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
