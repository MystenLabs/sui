// Make sure that legal usage is allowed
module 0x1::M {
    // verify-only struct
    #[spec_only]
    public struct Foo {}

    public fun foo() {
    }

    // verify-only struct used in a verify-only function
    #[spec_only]
    public fun bar(): Foo {
        Foo {}
    }

    // verify-only function used in a verify-only function
    #[spec_only]
    public fun baz(): Foo {
        bar()
    }
}
