module a::edge_cases {
    use sui::another::UID as AnotherUID;

    // Test case with a different UID type
    struct DifferentUID {
        id: AnotherUID,
    }

}

module sui::object {
    struct UID has store {
        id: address,
    }
}

module sui::another {
    struct UID has store {
        id: address,
    }
}
