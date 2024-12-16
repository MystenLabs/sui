// Check that spec_only filtering and calling is supported across modules and
// different types of module members
module 0x1::A {
    #[spec_only]
    public struct Foo has drop {}

    #[spec_only]
    public fun build_foo(): Foo {
        Foo {}
    }
}

module 0x1::B {
    #[spec_only]
    use 0x1::A::{Self, Foo};

    #[spec_only]
    fun x(_: Foo) {
    }

    #[spec_only]
    fun tester() {
        x(A::build_foo())
    }
}
