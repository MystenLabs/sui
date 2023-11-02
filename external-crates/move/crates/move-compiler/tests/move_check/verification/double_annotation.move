address 0x1 {
module M {
    #[verify_only]
    struct Foo {}

    // failure: double annotation
    #[verify_only]
    #[verify_only]
    struct Bar {}

    public fun foo() { }

    #[verify_only]
    public fun bar() { }

    // failure: double annotation
    #[verify_only]
    #[verify_only]
    public fun d(_a: signer, _b: signer) { }
}
}
