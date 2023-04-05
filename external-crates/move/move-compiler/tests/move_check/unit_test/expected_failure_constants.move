module 0x2::m {
    const C: u64 = 0 + 1 + 2;
}

// check that constants can be used
module 0x1::A {
    #[test_only]
    use 0x2::m as x;

    const C0: u64 = 0;
    #[test_only]
    const C1: u64 = 0;

    #[test]
    #[expected_failure(abort_code=C0)]
    fun use_c0() { }

    #[test]
    #[expected_failure(abort_code=C1)]
    fun use_c1() { }

    #[test]
    #[expected_failure(abort_code=x::C)]
    fun use_through_alias() { }

    #[test]
    #[expected_failure(abort_code=0x1::A::C0)]
    fun use_explicit_internal() { }

    #[test]
    #[expected_failure(abort_code=0x2::m::C)]
    fun use_explicit_external() { }
}
