module a::m{
    public fun f(): u64 { 0 }
}

#[test_only]
extend module a::m {
    public fun g(): u64 { 1 }
}

#[test_only]
extend module a::m {
    public fun g(): u64 { 1 }
}

