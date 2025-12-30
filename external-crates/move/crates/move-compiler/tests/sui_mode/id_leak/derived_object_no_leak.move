// valid because we can use `derived_object::claim` without triggering id leak
module a::m {
    use sui::derived_object;
    use sui::object;

    struct A has key {
        id: object::UID,
    }

    public fun no_leak(ctx: &mut sui::tx_context::TxContext): A {
        A {
            id: derived_object::claim(object::new(ctx), 0u64),
        }
    }
}

module sui::object {
    struct UID has store {
        id: address,
    }

    public fun new(_: &mut sui::tx_context::TxContext): UID {
        abort 0
    }
}

module sui::tx_context {
    struct TxContext has drop {}
}

module sui::derived_object {
    use sui::object::UID;

    public fun claim<T: copy + store + drop>(_: UID, _: T): UID {
        abort 0
    }
}
