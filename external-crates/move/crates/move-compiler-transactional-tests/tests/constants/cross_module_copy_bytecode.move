//# init --edition 2024.alpha

// We double-check that strange name collision cases do not crop up

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
