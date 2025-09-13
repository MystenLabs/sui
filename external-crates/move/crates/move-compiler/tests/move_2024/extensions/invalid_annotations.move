module a::m {
    public fun f(): u64 { 0 }
}

#[mode(test)]
#[allow(unreachable_code)]
extend module a::m {
    public fun g(): u64 { 1 }
}
