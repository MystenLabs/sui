module prover::core {
    native public fun assert_(p: bool);
}

module prover::types {
    native struct Integer has copy, drop, store;
    native struct Real has copy, drop, store;
}

module prover::integer {
    use prover::types::Integer;
    use prover::types::Real;
    native public fun from_u8(x: u8): Integer;
    native public fun from_u16(x: u16): Integer;
    native public fun from_u32(x: u32): Integer;
    native public fun from_u64(x: u64): Integer;
    native public fun to_real(x: Integer): Real;
    native public fun add(x: Integer, y: Integer): Integer;
    native public fun sub(x: Integer, y: Integer): Integer;
    native public fun neg(x: Integer): Integer;
    native public fun mul(x: Integer, y: Integer): Integer;
    native public fun idiv(x: Integer, y: Integer): Integer;
    native public fun div(x: Integer, y: Integer): Real;
    native public fun mod(x: Integer, y: Integer): Integer;
    native public fun eq(x: Integer, y: Integer): bool;
    native public fun lt(x: Integer, y: Integer): bool;
    native public fun gt(x: Integer, y: Integer): bool;
    native public fun lte(x: Integer, y: Integer): bool;
    native public fun gte(x: Integer, y: Integer): bool;
}

module prover::real {
    use prover::types::Integer;
    use prover::types::Real;
    native public fun to_integer(x: Real): Integer;
    native public fun add(x: Real, y: Real): Real;
    native public fun sub(x: Real, y: Real): Real;
    native public fun neg(x: Real): Real;
    native public fun mul(x: Real, y: Real): Real;
    native public fun div(x: Real, y: Real): Real;
    native public fun eq(x: Real, y: Real): bool;
    native public fun lt(x: Real, y: Real): bool;
    native public fun gt(x: Real, y: Real): bool;
    native public fun lte(x: Real, y: Real): bool;
    native public fun gte(x: Real, y: Real): bool;
}
