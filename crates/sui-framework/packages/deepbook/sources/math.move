// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module deepbook::math {
    /// scaling setting for float
    const FLOAT_SCALING: u64 = 1_000_000_000;
    const FLOAT_SCALING_U128: u128 = 1_000_000_000;

    friend deepbook::clob;


    // <<<<<<<<<<<<<<<<<<<<<<<< Error codes <<<<<<<<<<<<<<<<<<<<<<<<
    const EUnderflow: u64 = 1;
    // <<<<<<<<<<<<<<<<<<<<<<<< Error codes <<<<<<<<<<<<<<<<<<<<<<<<

    // multiply two floating numbers
    public(friend) fun unsafe_mul(x: u64, y: u64): u64 {
        let (_, result) = unsafe_mul_round(x, y);
        result
    }

    // multiply two floating numbers
    // also returns whether the result is rounded down
    public(friend) fun unsafe_mul_round(x: u64, y: u64): (bool, u64) {
        let x = (x as u128);
        let y = (y as u128);
        let is_round_down = true;
        if ((x * y) % FLOAT_SCALING_U128 == 0) is_round_down = false;
        (is_round_down, ((x * y / FLOAT_SCALING_U128) as u64))
    }

    // multiply two floating numbers and assert the result is non zero
    public fun mul(x: u64, y: u64): u64 {
        let (_, result) = unsafe_mul_round(x, y);
        assert!(result > 0, EUnderflow);
        result
    }

    // multiply two floating numbers and assert the result is non zero
    // also returns whether the result is rounded down
    public fun mul_round(x: u64, y: u64): (bool, u64) {
        let (is_round_down, result) = unsafe_mul_round(x, y);
        assert!(result > 0, EUnderflow);
        (is_round_down, result)
    }

    // divide two floating numbers
    // also returns whether the result is rounded down
    public(friend) fun unsafe_div_round(x: u64, y: u64): (bool, u64) {
        let x = (x as u128);
        let y = (y as u128);
        let is_round_down = true;
        if ((x * (FLOAT_SCALING as u128) % y) == 0) is_round_down = false;
        (is_round_down, ((x * (FLOAT_SCALING as u128) / y) as u64))
    }

    // divide two floating numbers and assert the result is non zero
    // also returns whether the result is rounded down
    public fun div_round(x: u64, y: u64): (bool, u64) {
        let (is_round_down, result) = unsafe_div_round(x, y);
        assert!(result > 0, EUnderflow);
        (is_round_down, result)
    }

    #[test_only] use sui::test_utils::assert_eq;

    #[test]
    fun test_mul() {
        assert_eq(unsafe_mul(1_000_000_000, 1), 1);
        assert_eq(unsafe_mul(9_999_999_999, 1), 9);
        assert_eq(unsafe_mul(9_000_000_000, 1), 9);
    }

    #[test]
    #[expected_failure(abort_code = EUnderflow)]
    fun test_mul_underflow() {
        mul(99_999_999, 1);
    }

    #[test]
    #[expected_failure(abort_code = EUnderflow)]
    fun test_mul_round_check_underflow() {
        mul_round(99_999_999, 1);
    }

    #[test]
    fun test_mul_round() {
        let (is_round, result) = unsafe_mul_round(1_000_000_000, 1);
        assert_eq(is_round, false);
        assert_eq(result, 1);
        (is_round, result) = unsafe_mul_round(9_999_999_999, 1);
        assert_eq(is_round, true);
        assert_eq(result, 9);
    }

    #[test]
    fun test_div_round() {
        let (is_round, result) = unsafe_div_round(1, 1_000_000_000);
        assert_eq(is_round, false);
        assert_eq(result, 1);
        (is_round, result) = unsafe_div_round(1, 9_999_999_999);
        assert_eq(is_round, true);
        assert_eq(result, 0);
        (is_round, result) = unsafe_div_round(1, 999_999_999);
        assert_eq(is_round, true);
        assert_eq(result, 1);
    }

    #[test]
    #[expected_failure(abort_code = EUnderflow)]
    fun test_div_round_check_underflow() {
        div_round(1, 1_000_000_001);
    }
}
