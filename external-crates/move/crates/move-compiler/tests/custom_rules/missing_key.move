module a::m {
    use sui::object::UID;

    struct MissingKeyAbility {
        id: UID,
    }

    // This struct is compliant and should not trigger the linter warning.
    // It has the 'key' ability as required.
    struct WithKeyAbility has key {
        id: UID,
    }

}