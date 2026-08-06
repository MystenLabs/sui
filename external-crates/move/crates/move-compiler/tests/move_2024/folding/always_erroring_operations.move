// operations over constants that always error at runtime should be warned about

module 0x42::m {
    const MAX_U16: u16 = 0xFFFF;
    const ZERO: u64 = 0;

    public fun div(): u64 { 1u64 / 0 }

    public fun div_constant(): u64 { 1u64 / ZERO }

    public fun mod_(): u64 { 1u64 % 0 }

    public fun add(): u8 { 255u8 + 1u8 }

    public fun sub(): u8 { 0u8 - 1u8 }

    public fun mul(): u8 { 128u8 * 2u8 }

    public fun shl(): u8 { 1u8 << 8 }

    public fun shr(): u16 { 1u16 >> 16 }

    public fun cast(): u8 { 256u64 as u8 }

    public fun cast_constant(): u8 { MAX_U16 as u8 }

    // only the inner operation is reported
    public fun nested(): u64 { (1u64 / 0) + 1 }

    // the local is folded into the operation before it is checked
    public fun through_local(): u64 {
        let x = 0;
        1u64 / x
    }

    // reported even when the operation is conditionally evaluated
    public fun conditional(cond: bool): u64 { if (cond) 1u64 / 0 else 0 }

    // not reported, the code is removed before the check
    public fun unreachable(): u64 { if (false) 1u64 / 0 else 0 }

    #[allow(always_errors)]
    public fun suppressed(): u64 { 1u64 / 0 }
}
