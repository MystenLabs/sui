module a::m {
    public fun f(): u64 { 0 }
}

#[test_only]
extend module a {
    public fun g(): u64 { 1 }
}

#[mode(fuzzing)]
extend module m {
    public fun g(): u64 { 1 }
}

