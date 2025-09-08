module a::m {
    fun f(): u64 { 42 }
}

#[test_only]
extend module a::m {
    fun g(): u64 { 24 }

    #[test]
    fun test() {
        assert!(f() == 42, 1);
        assert!(g() == 24, 2);
    }
}
