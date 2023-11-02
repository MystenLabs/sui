// invalid as NoStore doesn't have store, so Obj doesn't have key

module a::m {
    use sui::object;

    struct Obj<T> has key { id: object::UID, value: T }
    struct NoStore has copy, drop { value: u64 }

    public entry fun t1(_: Obj<NoStore>) {
        abort 0
    }

    // valid, while T doesn't have store, and might it later, we require it to be annotated
    public entry fun t2<T>(_: Obj<T>) {
        abort 0
    }

}

module sui::object {
    struct UID has store {
        id: address,
    }
}
