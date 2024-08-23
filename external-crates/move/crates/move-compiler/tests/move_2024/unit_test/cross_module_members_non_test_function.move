// check that `use`'s are filtered out correctly in non-test mode
module 0x1::P {
    public struct Foo has drop {}

    public fun build_foo(): Foo { Foo {} }
}

module 0x1::Q {
    #[test_only]
    use 0x1::P::{Self, Foo};

    #[test_only]
    fun x(_: Foo) { }

    #[test]
    fun tester() {
        x(P::build_foo())
    }

    // this should fail find the P module in non-test mode as the use statement
    // because `P` is test_only.
    public fun bad(): Foo {
        P::build_foo()
    }
}
