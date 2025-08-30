module a::m{
    public fun f(): u64 { 0 }
}

#[test_only]
extend asdf::m {
    public fun g(): u64 { 1 }
}

