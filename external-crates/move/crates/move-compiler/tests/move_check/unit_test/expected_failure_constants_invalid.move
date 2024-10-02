module 0x2::m {
    const C: u8 = 0 + 1 + 2;
}

// check invalid constant usage
module 0x1::A {

    #[test]
    #[expected_failure(abort_code=0x2::m::C)]
    fun not_u64() { }

    #[test]
    #[expected_failure(abort_code=0x2::x::C)]
    fun unbound_module() { }

    #[test]
    #[expected_failure(abort_code=0x1::A::C0)]
    fun unbound_constant() { }
}
