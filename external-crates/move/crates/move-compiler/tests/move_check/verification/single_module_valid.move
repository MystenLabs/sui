// Make sure that legal usage is allowed
module 0x1::M {
    // verify-only struct
    #[verify_only]
    struct Foo {}

    public fun foo() { }

    // verify-only struct used in a verify-only function
    #[verify_only]
    public fun bar(): Foo { Foo{} }

    // verify-only function used in a verify-only function
    #[verify_only]
    public fun baz(): Foo { bar() }
}
