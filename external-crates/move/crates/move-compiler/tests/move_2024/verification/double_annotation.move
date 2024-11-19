module 0x1::M {
    #[spec_only]
    public struct Foo {}

    // failure: double annotation
    #[spec_only]
    #[spec_only]
    public struct Bar {}

    public fun foo() {
    }

    #[spec_only]
    public fun bar() {
    }

    // failure: double annotation
    #[spec_only]
    #[spec_only]
    public fun d(_a: signer, _b: signer) {
    }
}
