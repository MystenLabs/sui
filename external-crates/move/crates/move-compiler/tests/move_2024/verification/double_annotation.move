address 0x1 {
module M {
    #[spec_only]
    struct Foo {}

    // failure: double annotation
    #[spec_only]
    #[spec_only]
    struct Bar {}

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
}
