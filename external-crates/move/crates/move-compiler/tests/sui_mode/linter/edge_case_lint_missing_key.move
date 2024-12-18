module a::edge_cases {
    struct UID {}
    // Test case with a different UID type
    struct DifferentUID {
        id: sui::another::UID,
    }

    struct NotAnObject {
        id: UID,
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
