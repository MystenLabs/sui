module a::m {
    fun f(): u64 { 42 }
}

extend module a::m {
    fun g(): u64 { 24 }

    fun test() {
        assert!(f() == 42, 1);
        assert!(g() == 24, 2);
    }
}
