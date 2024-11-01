module a::trigger_lint_cases {
    use sui::object::UID;

    // This should trigger the linter warning (true positive)
    struct MissingKeyAbility {
        id: UID,
    }

}

module sui::object {
    struct UID has store {
        id: address,
    }
}
