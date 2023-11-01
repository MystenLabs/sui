
// tests that the example which is allowed in sui mode is not allowed outside of that mode

module a::m {
    struct Obj has key { id: sui::object::UID }
}

module sui::object {
    struct UID has store { value: address }
    public fun borrow_address(id: &UID): &address { &id.value }
}
