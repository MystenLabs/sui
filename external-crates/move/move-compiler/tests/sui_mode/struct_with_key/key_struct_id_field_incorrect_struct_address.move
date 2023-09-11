// invalid, the object has a UID field, but not the sui::object::UID
module a::object {
    struct UID has store { flag: bool }
    struct S has key {
        id: UID
    }
}

// TODO we might want to support this
module 0x2::object {
    struct UID has store { flag: bool }
    struct S has key {
        id: UID
    }
}
