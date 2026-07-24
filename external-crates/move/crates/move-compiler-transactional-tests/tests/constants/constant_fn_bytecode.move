//# init --edition 2024.alpha

// The generated constant functions are on demand: '_const_USED' exists in 0x42::a because
// 0x42::b reads it, while 'LOCAL_ONLY' gets no function since it is only used in its defining
// module

//# print-bytecode
module 0x42::a {
    public(package) const USED: u64 = 1;
    public(package) const LOCAL_ONLY: u64 = 2;

    public fun local(): u64 { LOCAL_ONLY }
}

module 0x42::b {
    use 0x42::a;

    public fun used(): u64 { a::USED }
}
