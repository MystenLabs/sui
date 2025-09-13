module a::m {
    fun f(): u64 { 0 }
}

#[test_only]
extend module a::m {
    fun f(): u64 { 1 }
}
