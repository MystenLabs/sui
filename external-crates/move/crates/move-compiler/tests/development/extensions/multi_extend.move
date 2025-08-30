module a::m {
    public fun f(): u64 { 0 }
}

#[mode(test)]
extend a::m {
    public fun g(): u64 { 0 }
}

#[mode(spec)]
extend a::m {
    public fun g(): u64 { 0 }
}
