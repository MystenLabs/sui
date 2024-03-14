module a::a {
    const C: u64 = 0 + 1 + 2;
    public fun foo() {}
}

#[test_only]
module 0x1::c {
    use a::a;
    #[test]
    #[expected_failure(abort_code=a::a::C)]
    fun use_explicit_external_named() {
        a::foo()
    }
}
