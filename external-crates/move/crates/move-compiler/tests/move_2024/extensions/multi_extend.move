module a::m {
    public fun f(): u64 { 0 }
}

#[mode(test)]
extend module a::m {
    public fun g(): u64 { 0 }
}

#[mode(spec)]
extend module a::m {
    public fun g(): u64 { 0 }
}
