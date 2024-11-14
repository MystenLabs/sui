module std::real;

#[spec_only]
use std::integer::Integer;

#[spec_only]
native public struct Real has copy, drop, store;

#[spec_only]
native public fun from_integer(x: Integer): Real;
#[spec_only]
native public fun to_integer(x: Real): Integer;

#[spec_only]
public fun from_u8(x: u8): Real {
    x.to_int().to_real()
}
#[spec_only]
public fun from_u16(x: u16): Real {
    x.to_int().to_real()
}
#[spec_only]
public fun from_u32(x: u32): Real {
    x.to_int().to_real()
}
#[spec_only]
public fun from_u64(x: u64): Real {
    x.to_int().to_real()
}
#[spec_only]
public fun from_u128(x: u128): Real {
    x.to_int().to_real()
}
#[spec_only]
public fun from_u256(x: u256): Real {
    x.to_int().to_real()
}

#[spec_only]
public fun to_u8(x: Real): u8 {
    x.to_integer().to_u8()
}
#[spec_only]
public fun to_u16(x: Real): u16 {
    x.to_integer().to_u16()
}
#[spec_only]
public fun to_u32(x: Real): u32 {
    x.to_integer().to_u32()
}
#[spec_only]
public fun to_u64(x: Real): u64 {
    x.to_integer().to_u64()
}
#[spec_only]
public fun to_u128(x: Real): u128 {
    x.to_integer().to_u128()
}
#[spec_only]
public fun to_u256(x: Real): u256 {
    x.to_integer().to_u256()
}

#[spec_only]
native public fun add(x: Real, y: Real): Real;
#[spec_only]
native public fun sub(x: Real, y: Real): Real;
#[spec_only]
native public fun neg(x: Real): Real;
#[spec_only]
native public fun mul(x: Real, y: Real): Real;
#[spec_only]
native public fun div(x: Real, y: Real): Real;
#[spec_only]
native public fun sqrt(x: Real, y: Real): Real;
#[spec_only]
native public fun exp(x: Real, y: Real): Real;

#[spec_only]
native public fun lt(x: Real, y: Real): bool;
#[spec_only]
native public fun gt(x: Real, y: Real): bool;
#[spec_only]
native public fun lte(x: Real, y: Real): bool;
#[spec_only]
native public fun gte(x: Real, y: Real): bool;
