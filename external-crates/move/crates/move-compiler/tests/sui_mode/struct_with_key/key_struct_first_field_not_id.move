// invalid, first field of an object must be sui::object::UID
module a::m {
    struct S has key {
        flag: bool,
    }
}
