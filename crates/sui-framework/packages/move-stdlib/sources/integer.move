module std::integer {
    #[verify_only]
    native public struct Integer has copy, drop, store;

    #[verify_only]
    native public fun from_u8(x: u8): Integer;
    #[verify_only]
    native public fun from_u16(x: u16): Integer;
    #[verify_only]
    native public fun from_u32(x: u32): Integer;
    #[verify_only]
    native public fun from_u64(x: u64): Integer;
    #[verify_only]
    native public fun from_u128(x: u128): Integer;
    #[verify_only]
    native public fun from_u256(x: u256): Integer;

    #[verify_only]
    native public fun to_u8(x: Integer): u8;
    #[verify_only]
    native public fun to_u16(x: Integer): u16;
    #[verify_only]
    native public fun to_u32(x: Integer): u32;
    #[verify_only]
    native public fun to_u64(x: Integer): u64;
    #[verify_only]
    native public fun to_u128(x: Integer): u128;
    #[verify_only]
    native public fun to_u256(x: Integer): u256;

    #[verify_only]
    public use fun std::real::from_integer as Integer.to_real;

    #[verify_only]
    native public fun add(x: Integer, y: Integer): Integer;
    #[verify_only]
    native public fun sub(x: Integer, y: Integer): Integer;
    #[verify_only]
    native public fun neg(x: Integer): Integer;
    #[verify_only]
    native public fun mul(x: Integer, y: Integer): Integer;
    #[verify_only]
    native public fun div(x: Integer, y: Integer): Integer;
    #[verify_only]
    native public fun mod(x: Integer, y: Integer): Integer;
    #[verify_only]
    native public fun sqrt(x: Integer, y: Integer): Integer;
    #[verify_only]
    native public fun pow(x: Integer, y: Integer): Integer;

    #[verify_only]
    public fun shl(x: Integer, y: Integer): Integer {
        x.mul(2u8.to_int().pow(y))
    }
    #[verify_only]
    public fun shr(x: Integer, y: Integer): Integer {
        x.div(2u8.to_int().pow(y))
    }

    #[verify_only]
    native public fun lt(x: Integer, y: Integer): bool;
    #[verify_only]
    native public fun gt(x: Integer, y: Integer): bool;
    #[verify_only]
    native public fun lte(x: Integer, y: Integer): bool;
    #[verify_only]
    native public fun gte(x: Integer, y: Integer): bool;
}
