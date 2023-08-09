// allowed, no object is being made with the UID
module a::m {
    use sui::object::UID;

    struct Foo has key {
        id: UID,
    }

    public fun foo(f: Foo): UID {
        let Foo { id } = f;
        id
    }
}

module sui::object {
    struct UID has store {
        id: address,
    }
}
