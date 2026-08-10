//# init --edition 2024.alpha

// Constant copies are synthesized on demand in each using module: 0x42::b gets copies of
// 'USED' and 'BYTES', 0x42::c gets its own copy of 'USED', and 'LOCAL_ONLY' is copied
// nowhere. Copy names lead with the defining module's id in the compilation's module list, so
// the constants of 'x' and 'x_A' -- which would both mangle to '_x_A_B' under module-name-only
// mangling -- get distinct names in 0x42::user

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

module 0x42::x {
    public(package) const A_B: u64 = 3;
}

module 0x42::x_A {
    public(package) const B: u64 = 4;
}

module 0x42::user {
    use 0x42::x;
    use 0x42::x_A;

    public fun both(): u64 { x::A_B + x_A::B }
}
