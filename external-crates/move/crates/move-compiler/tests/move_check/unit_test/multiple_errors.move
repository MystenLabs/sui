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

    // failure: invalid args to test attribute
    #[test(_a=Foo)]
    public fun b() { }

    // failure: double annotation
    #[test]
    #[test]
    public fun d() { }

    // failure: annotated as both test and test_only
    #[test]
    #[test_only]
    public fun e() { }

    // failure: invalid number of test arguments
    #[test]
    public fun f(_x: u64) { }

    // failure: double annotation
    #[test]
    #[expected_failure]
    #[expected_failure]
    public fun g() { }
}
}
