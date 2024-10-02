// check that `use`'s are filtered out correctly
address 0x1 {
module A {
    struct Foo has drop {}

    public fun build_foo(): Foo { Foo {} }
}

module B {
    #[verify_only]
    use 0x1::A::{Self, Foo};

    #[verify_only]
    fun x(_: Foo) { }

    #[verify_only]
    fun tester() {
        x(A::build_foo())
    }

    // this should fail
    public fun bad(): Foo {
        A::build_foo()
    }
}
}
