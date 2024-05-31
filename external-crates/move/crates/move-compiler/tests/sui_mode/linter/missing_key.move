module a::m {
    use sui::object::UID;

    // This should trigger the linter warning (true positive)
    struct MissingKeyAbility {
        id: UID,
    }

    // This should not trigger the linter warning (true negative)
    struct HasKeyAbility has key {
        id: UID,
    }

    // False negative cases (should trigger warning but might not):

    // 1. Different field name
    struct FN1_MissingKeyWithDifferentFieldName {
        uid: UID,
    }

    // 2. UID field not first
    struct FN2_MissingKeyUIDNotFirst {
        point: u64,
        id: UID,
    }

    // 3. Nested UID
    struct FN3_MissingKeyNestedUID {
        wrapper: Wrapper,
    }

    struct Wrapper {
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
