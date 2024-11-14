module std::integer;

#[spec_only]
native public struct Integer has copy, drop, store;

#[spec_only]
native public fun from_u8(x: u8): Integer;
#[spec_only]
native public fun from_u16(x: u16): Integer;
#[spec_only]
native public fun from_u32(x: u32): Integer;
#[spec_only]
native public fun from_u64(x: u64): Integer;
#[spec_only]
native public fun from_u128(x: u128): Integer;
#[spec_only]
native public fun from_u256(x: u256): Integer;

#[spec_only]
native public fun to_u8(x: Integer): u8;
#[spec_only]
native public fun to_u16(x: Integer): u16;
#[spec_only]
native public fun to_u32(x: Integer): u32;
#[spec_only]
native public fun to_u64(x: Integer): u64;
#[spec_only]
native public fun to_u128(x: Integer): u128;
#[spec_only]
native public fun to_u256(x: Integer): u256;

#[spec_only]
public use fun std::real::from_integer as Integer.to_real;

#[spec_only]
native public fun add(x: Integer, y: Integer): Integer;
#[spec_only]
native public fun sub(x: Integer, y: Integer): Integer;
#[spec_only]
native public fun neg(x: Integer): Integer;
#[spec_only]
native public fun mul(x: Integer, y: Integer): Integer;
#[spec_only]
native public fun div(x: Integer, y: Integer): Integer;
#[spec_only]
native public fun mod(x: Integer, y: Integer): Integer;
#[spec_only]
native public fun sqrt(x: Integer, y: Integer): Integer;
#[spec_only]
native public fun pow(x: Integer, y: Integer): Integer;

#[spec_only]
public fun shl(x: Integer, y: Integer): Integer {
    x.mul(2u8.to_int().pow(y))
}
#[spec_only]
public fun shr(x: Integer, y: Integer): Integer {
    x.div(2u8.to_int().pow(y))
}

#[spec_only]
native public fun lt(x: Integer, y: Integer): bool;
#[spec_only]
native public fun gt(x: Integer, y: Integer): bool;
#[spec_only]
native public fun lte(x: Integer, y: Integer): bool;
#[spec_only]
native public fun gte(x: Integer, y: Integer): bool;
