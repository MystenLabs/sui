module a::m {
    public fun f(): u64 { 0 }
}

#[test_only]
extend address a {
    module m {
        public fun g(): u64 { 1 }
    }
}

