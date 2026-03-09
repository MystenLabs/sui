module a::test {
    use sui::object::UID;

    struct S has key {
        id: UID,
    }

    fun make(): S {
        S {}
    }
}

module sui::object {
    struct UID has store {
        id: address,
    }
}

module sui::transfer {
    public fun transfer<T: key>(_: T, _: address) {
        abort 0
    }
}
