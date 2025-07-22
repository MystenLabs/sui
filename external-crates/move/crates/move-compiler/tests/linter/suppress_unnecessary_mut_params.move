module 0x42::suppressions {
    use sui::object::UID;

    struct Person has key, store {
        id: UID,
        count: u64
    }

    // Case 1: Basic suppression
    #[allow(lint(unnecessary_mut_params))]
    public fun suppressed_unused_mut(person: &mut Person) {
        let _count = person.count;
    }

    // Case 2: Multiple suppressions
    #[allow(lint(unnecessary_mut_params))]
    public fun multiple_suppressions(person: &mut Person, other: &mut Person) {
        let _sum = person.count + other.count;
    }

    // Case 3: Suppressed with documentation
    /// This function takes a mutable reference for future extensibility
    #[allow(lint(unnecessary_mut_params))]
    public fun documented_suppression(person: &mut Person) {
        let _count = person.count;
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
