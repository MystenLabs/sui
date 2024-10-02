#[test_only]
address 0x42 {
module A {
    struct T {}
}
}

#[test_only]
module 0x42::M {
}

module 0x42::N {
    #[test_only]
    friend 0x42::M;

    #[test_only]
    use 0x42::A;

    #[test_only]
    const C: u64 = 0;

    #[test_only]
    struct S { f: A::T }

    #[test_only]
    fun foo() {}
}

module 0x42::Z {
    #[test]
    #[expected_failure(abort_code = 0, location = 0x42::Z )]
    fun foo() { abort 0 }
}
