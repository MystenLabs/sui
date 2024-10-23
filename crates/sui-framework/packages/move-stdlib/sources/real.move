// #[test_only]
module std::real {
    use std::integer::Integer;

    native public struct Real has copy, drop, store;

    native public fun from_integer(x: Integer): Real;
    native public fun to_integer(x: Real): Integer;

    public fun from_u8(x: u8): Real {
        x.to_int().to_real()
    }
    public fun from_u16(x: u16): Real {
        x.to_int().to_real()
    }
    public fun from_u32(x: u32): Real {
        x.to_int().to_real()
    }
    public fun from_u64(x: u64): Real {
        x.to_int().to_real()
    }
    public fun from_u128(x: u128): Real {
        x.to_int().to_real()
    }
    public fun from_u256(x: u256): Real {
        x.to_int().to_real()
    }

    public fun to_u8(x: Real): u8 {
        x.to_integer().to_u8()
    }
    public fun to_u16(x: Real): u16 {
        x.to_integer().to_u16()
    }
    public fun to_u32(x: Real): u32 {
        x.to_integer().to_u32()
    }
    public fun to_u64(x: Real): u64 {
        x.to_integer().to_u64()
    }
    public fun to_u128(x: Real): u128 {
        x.to_integer().to_u128()
    }
    public fun to_u256(x: Real): u256 {
        x.to_integer().to_u256()
    }

    native public fun add(x: Real, y: Real): Real;
    native public fun sub(x: Real, y: Real): Real;
    native public fun neg(x: Real): Real;
    native public fun mul(x: Real, y: Real): Real;
    native public fun div(x: Real, y: Real): Real;
    native public fun sqrt(x: Real, y: Real): Real;
    native public fun exp(x: Real, y: Real): Real;

    native public fun lt(x: Real, y: Real): bool;
    native public fun gt(x: Real, y: Real): bool;
    native public fun lte(x: Real, y: Real): bool;
    native public fun gte(x: Real, y: Real): bool;
}
