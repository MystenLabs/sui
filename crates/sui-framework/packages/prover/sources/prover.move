module prover::prover {
    native public fun assume_(p: bool);
    native public fun assert_(p: bool);
    // native public fun invariant_(p: bool);

    public fun implies(p: bool, q: bool): bool {
        !p || q
    }

    const MAX_U8: u8 = 255u8;
    const MAX_U16: u16 = 65535u16;
    const MAX_U32: u32 = 4294967295u32;
    const MAX_U64: u64 = 18446744073709551615u64;
    const MAX_U128: u128 = 340282366920938463463374607431768211455u128;
    const MAX_U256: u256 = 115792089237316195423570985008687907853269984665640564039457584007913129639935u256;
    public fun max_u8(): u8 {
        MAX_U8
    }
    public fun max_u16(): u16 {
        MAX_U16
    }
    public fun max_u32(): u32 {
        MAX_U32
    }
    public fun max_u64(): u64 {
        MAX_U64
    }
    public fun max_u128(): u128 {
        MAX_U128
    }
    public fun max_u256(): u256 {
        MAX_U256
    }
}

module prover::integer {
    native public struct Integer has copy, drop, store;

    // const MAX_U64: Integer = 18446744073709551615;

    // use macro to template over u8/u16/u32/u64/u128/u256 if possible?
    native public fun from_u8(x: u8): Integer;
    native public fun from_u16(x: u16): Integer;
    native public fun from_u32(x: u32): Integer;
    native public fun from_u64(x: u64): Integer;
    native public fun from_u128(x: u128): Integer;
    native public fun from_u256(x: u256): Integer;
    native public fun to_u8(x: Integer): u8;
    native public fun to_u16(x: Integer): u16;
    native public fun to_u32(x: Integer): u32;
    native public fun to_u64(x: Integer): u64;
    native public fun to_u128(x: Integer): u128;
    native public fun to_u256(x: Integer): u256;
    public use fun prover::real::from_integer as Integer.to_real;
    native public fun add(x: Integer, y: Integer): Integer;
    native public fun sub(x: Integer, y: Integer): Integer;
    native public fun neg(x: Integer): Integer;
    native public fun mul(x: Integer, y: Integer): Integer;
    native public fun div(x: Integer, y: Integer): Integer;
    native public fun mod(x: Integer, y: Integer): Integer;
    native public fun pow(x: Integer, y: Integer): Integer;
    public fun shl(x: Integer, y: Integer): Integer {
        mul(x, pow(from_u8(2), y))
    }
    public fun shr(x: Integer, y: Integer): Integer {
        div(x, pow(from_u8(2), y))
    }
    native public fun eq(x: Integer, y: Integer): bool;
    native public fun lt(x: Integer, y: Integer): bool;
    native public fun gt(x: Integer, y: Integer): bool;
    native public fun lte(x: Integer, y: Integer): bool;
    native public fun gte(x: Integer, y: Integer): bool;
    // native public fun div_real(x: Integer, y: Integer): Real;
}

module prover::real {
    use prover::integer::Integer;
    native public struct Real has copy, drop, store;
    native public fun from_integer(x: Integer): Real;
    native public fun to_integer(x: Real): Integer;
    native public fun add(x: Real, y: Real): Real;
    native public fun sub(x: Real, y: Real): Real;
    native public fun neg(x: Real): Real;
    native public fun mul(x: Real, y: Real): Real;
    native public fun div(x: Real, y: Real): Real;
    native public fun exp(x: Real, y: Real): Real;
    native public fun eq(x: Real, y: Real): bool;
    native public fun lt(x: Real, y: Real): bool;
    native public fun gt(x: Real, y: Real): bool;
    native public fun lte(x: Real, y: Real): bool;
    native public fun gte(x: Real, y: Real): bool;
}
