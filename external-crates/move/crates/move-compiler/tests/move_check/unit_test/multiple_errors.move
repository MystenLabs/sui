// test that we handle multiple errors in the same file correctly and don't stop at the first one
address 0x1 {
module M {
    #[test_only]
    struct Foo {}

    public fun foo() { }

    #[test_only]
    public fun bar() { }

    #[test]
    public fun go() { }

    #[test(_a=@0x1)]
    public fun a(_a: address) { }

    // failure: invalid value in test parameter assignment
    #[test(_a=Foo)]
    public fun b(_a: address) { }

    #[test(_a=@0x1, _b=@0x2)]
    public fun c(_a: address, _b: address) { }

    // failure: double annotation
    #[test(_a=@0x1)]
    #[test(_b=@0x2)]
    public fun d(_a: address, _b: address) { }

    // failure: annotated as both test and test_only
    #[test(_a=@0x1)]
    #[test_only]
    public fun e(_a: address, _b: address) { }

    // failure: invalid number of address arguments
    #[test(_a=@0x1)]
    public fun f(_a: address, _b: address) { }

    // failure: double annotation
    #[test(_a=@0x1)]
    #[expected_failure]
    #[expected_failure]
    public fun g(_a: address, _b: address) { }
}
}
