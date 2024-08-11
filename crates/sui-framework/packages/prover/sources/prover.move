module prover::p {
    native public fun assert_(p: bool);

    native struct Integer has copy, drop, store;
    native public fun integer_from_u64(x: u64): Integer;
    native public fun integer_add(x: Integer, y: Integer): Integer;
    native public fun integer_lt(x: Integer, y: Integer): bool;
}
