// expected_failure attributes can only be placed on #[test] functions
module 0x1::A {
    #[expected_failure]
    struct Foo {}

    #[expected_failure]
    use 0x1::A;

    #[expected_failure]
    const C: u64 = 0;

    struct Bar { f: A::Foo }
}
