// invalid, id field must have type UID
module a::m {
    struct S has key {
        id: bool
    }
}
