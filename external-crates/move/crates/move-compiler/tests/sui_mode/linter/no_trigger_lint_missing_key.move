module a::no_trigger_lint_cases {
    use sui::object::UID;

    // This should not trigger the linter warning (true negative)
    struct HasKeyAbility has key {
        id: UID,
    }

    // False positive cases (should not trigger warning but might):

    // 1. Has key but different field name
    struct FP1_HasKeyButDifferentFieldName has key {
        uid: UID,
    }

    // 2. Has key but UID field not first
    struct FP2_HasKeyUIDNotFirst has key {
        point: u64,
        id: UID,
    }

    // 3. Has key with ID field of different type
    struct FP3_HasKeyButIDNotUID has key {
        id: address,
    }

    // 4. Suppress warning
    #[allow(lint(missing_key))]
    struct SuppressWarning {
       id: UID,
    }

    // 5. Has ability but not key
    struct FP5_HasAbilityButNotKey has store, copy, drop {
        id: UID,
    }
}

module sui::object {
    struct UID has store {
        id: address,
    }
}