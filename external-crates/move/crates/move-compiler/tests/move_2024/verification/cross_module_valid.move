// Check that spec_only filtering and calling is supported across modules and
// different types of module members
address 0x1 {
module A {
#[spec_only]
struct Foo has drop {}

#[spec_only]
public fun build_foo(): Foo {
    Foo {}
}
}


module B {
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
}
