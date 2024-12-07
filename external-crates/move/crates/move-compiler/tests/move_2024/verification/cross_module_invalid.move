// check that `use`'s are filtered out correctly
module 0x1::A {
    public struct Foo has drop {}

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

    // this should fail
    public fun bad(): Foo {
        A::build_foo()
    }
}
