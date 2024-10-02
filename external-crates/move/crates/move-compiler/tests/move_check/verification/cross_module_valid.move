// Check that verify_only filtering and calling is supported across modules and
// different types of module members
address 0x1 {
module A {
    #[verify_only]
    struct Foo has drop {}

    #[verify_only]
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
}
}
