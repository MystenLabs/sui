// invalid, the object has a UID field, but not the sui::object::UID
module a::object {
    struct UID has store { flag: bool }
    struct S has key {
        id: UID
    }
}

module 0x3::object {
    struct UID has store { flag: bool }
    struct S has key {
        id: UID
    }
}
