module 0x2::X {
    struct S {}
    public fun foo() {}
}

module 0x2::M {
    use 0x2::X::{Self as B, foo, S};

    struct X { f: X::S, f2: S }
    fun bar() {
        X::foo();
        foo()
    }
}
