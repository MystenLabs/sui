//# init --edition 2024.alpha

//# print-bytecode
#[allow(always_errors)] module 0x42::m {
    const ONE: u64 = 1;
    const TWO: u64 = 2;
    const MAX_U16: u16 = 0xFFFF;

    // operations over constants are folded, so the constants are never loaded
    public fun binop(): u64 { ONE + TWO }

    public fun cast(): u64 { MAX_U16 as u64 }

    public fun nested(): u64 { (MAX_U16 as u64) * (ONE + TWO) }

    // a constant used on its own is still loaded, not inlined
    public fun used_directly(): u64 { ONE }

    // the cast cannot be folded, so it remains and errors at runtime
    public fun unfoldable(): u8 { MAX_U16 as u8 }
}
