module a::m {
    fun f(): u64 { 42 }
}

#[test_only]
extend module a::n {
    fun g(): u64 { 24 }
}
