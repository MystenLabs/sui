// #[test_only]
module std::integer {
    native public struct Integer has copy, drop, store;

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

    public use fun std::real::from_integer as Integer.to_real;

    native public fun add(x: Integer, y: Integer): Integer;
    native public fun sub(x: Integer, y: Integer): Integer;
    native public fun neg(x: Integer): Integer;
    native public fun mul(x: Integer, y: Integer): Integer;
    native public fun div(x: Integer, y: Integer): Integer;
    native public fun mod(x: Integer, y: Integer): Integer;
    native public fun sqrt(x: Integer, y: Integer): Integer;
    native public fun pow(x: Integer, y: Integer): Integer;

    public fun shl(x: Integer, y: Integer): Integer {
        x.mul(2u8.to_int().pow(y))
    }
    public fun shr(x: Integer, y: Integer): Integer {
        x.div(2u8.to_int().pow(y))
    }

    native public fun lt(x: Integer, y: Integer): bool;
    native public fun gt(x: Integer, y: Integer): bool;
    native public fun lte(x: Integer, y: Integer): bool;
    native public fun gte(x: Integer, y: Integer): bool;

    // native public fun div_real(x: Integer, y: Integer): Real;
}
