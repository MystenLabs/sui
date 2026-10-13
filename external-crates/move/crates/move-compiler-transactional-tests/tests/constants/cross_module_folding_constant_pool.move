//# init --edition 2024.alpha

//# print-bytecode
module 0x42::a {
    public(package) const SEVEN: u64 = 7;
    public(package) const FLAG: bool = true;
    public(package) const MAX_U16: u16 = 0xFFFF;
    public(package) const H: u8 = 104;
    public(package) const I: u8 = 105;
    public(package) const HI: vector<u8> = b"hi";
}

// every use is folded, so no copies are synthesized
module 0x42::folded_only {
    use 0x42::a;

    public fun arith(): u64 { a::SEVEN + 1 }

    public fun branch(): u64 { if (a::FLAG) 1 else 2 }
}

// an unfoldable use still synthesizes a copy
module 0x42::unfoldable {
    use 0x42::a;

    #[allow(always_errors)]
    public fun cast(): u8 { a::MAX_U16 as u8 }
}

// a copy shares the pool entry of an equivalent constant
module 0x42::shared_entry {
    use 0x42::a;

    const LOCAL_SEVEN: u64 = 7;

    public fun both(): (u64, u64) { (LOCAL_SEVEN, a::SEVEN) }
}

// a folded vector reuses the pool entry of an equivalent expression with the same folded value
module 0x42::folded_vector {
    use 0x42::a;

    public fun copied(): vector<u8> { a::HI }

    public fun folded(): vector<u8> { vector[a::H, a::I] }
}

// a cross-module use in a constant definition does not synthesize a copy
module 0x42::in_constant {
    use 0x42::a;

    const EIGHT: u64 = a::SEVEN + 1;

    public fun eight(): u64 { EIGHT }
}
