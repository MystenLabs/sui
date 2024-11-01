module a::trigger_lint_cases {
    use sui::object::UID;

    // 4. Suppress warning
    #[allow(lint(missing_key))]
    struct SuppressWarning {
       id: UID,
    }
}

module sui::object {
    struct UID has store {
        id: address,
    }
}
