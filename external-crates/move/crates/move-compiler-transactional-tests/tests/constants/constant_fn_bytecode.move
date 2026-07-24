//# init --edition 2024.alpha

// Generated constant functions are on demand: '_const_USED' and '_const_BYTES' exist in
// 0x42::a because other modules read them, while 'LOCAL_ONLY' gets no function. Mixing a
// 'public(package)' function call with constant reads, and multiple reader modules, produces
// a single generated function per constant

//# print-bytecode
module 0x42::a {
    public(package) const USED: u64 = 1;
    public(package) const LOCAL_ONLY: u64 = 2;
    public(package) const BYTES: vector<u8> = b"hello";

    public(package) fun helper(): u64 { 0 }

    public fun local(): u64 { LOCAL_ONLY }
}

module 0x42::b {
    use 0x42::a;

    public fun both(): u64 { a::helper() + a::USED }

    public fun bytes(): vector<u8> { a::BYTES }
}

module 0x42::c {
    use 0x42::a;

    public fun read(): u64 { a::USED }
}
