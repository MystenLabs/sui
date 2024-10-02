// TxContext does not have to be present

module a::m {
    public entry fun t() {
        abort 0
    }

    struct Obj has key { id: sui::object::UID }
    public entry fun t2(_: bool, _: &mut Obj) {
        abort 0
    }
}

module sui::object {
    struct UID has store {
        id: address,
    }
}
