module 0x2::X {
    struct S {}
}

module 0x2::M {
    use 0x2::X::{Self, S as X};
    struct A { f1: X, f2: X::S }
}
