// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module std::u256_tests {
    use std::unit_test::assert_eq;

    const BIT_SIZE: u8 = 255;
    const MAX: u256 =
        0xFFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF;
    const MAX_PRED: u256 = MAX - 1;

    const CASES: vector<u256> = vector[
        0,
        1,
        10,
        11,
        100,
        111,
        1 << (BIT_SIZE / 2),
        (1 << (BIT_SIZE / 2)) + 1,
        1 << BIT_SIZE,
        (1 << BIT_SIZE) + 1,
        MAX / 2,
        (MAX / 2) + 1,
        MAX_PRED,
        MAX,
    ];

    macro fun custom_cases($cases: vector<_>, $f: |_, _, _|) {
        let mut cases = $cases;
        while (!cases.is_empty()) {
            let case = cases.pop_back();
            let case_pred = case.max(1) - 1;
            let case_succ = case.min(MAX_PRED) + 1;
            $f(case_pred, case, case_succ);
        }
    }

    macro fun cases($f: |_, _, _|) {
        custom_cases!(CASES, $f);
    }

    #[test]
    fun test_max() {
        let max = MAX;
        assert_eq!(max.max(max), max);
        cases!(|case_pred, case, case_succ| {
            assert_eq!(max.max(case), max);
            assert_eq!(case.max(max), max);
            assert_eq!(case.max(case), case);
            assert_eq!(case_pred.max(case), case);
            assert_eq!(case_succ.max(case), case_succ);
        })
    }

    #[test]
    fun test_min() {
        let max = MAX;
        assert_eq!(max.min(max), max);
        cases!(|case_pred, case, case_succ| {
            assert_eq!(max.min(case), case);
            assert_eq!(case.min(max), case);
            assert_eq!(case.min(case), case);
            assert_eq!(case_pred.min(case), case_pred);
            assert_eq!(case_succ.min(case), case);
        })
    }

    #[test]
    fun test_diff() {
        let max = MAX;
        assert_eq!(max.diff(max), 0);
        cases!(|case_pred, case, case_succ| {
            assert_eq!(max.diff(case), max - case);
            assert_eq!(case.diff(max), max - case);
            assert_eq!(case.diff(case), 0);
            assert_eq!(case_pred.diff(case), case - case_pred);
            assert_eq!(case.diff(case_pred), case - case_pred);
            assert_eq!(case_succ.diff(case), case_succ - case);
            assert_eq!(case.diff(case_succ), case_succ - case);
        })
    }

    macro fun check_div_round($x: _, $y: _) {
        let x = $x;
        let y = $y;
        if (y == 0) return;
        assert_eq!(x.divide_and_round_up(y), (x / y) + (x % y).min(1));
    }

    #[test]
    fun test_divide_and_round_up() {
        let max = MAX;
        assert_eq!(max.divide_and_round_up(max), 1);
        check_div_round!(max, max);
        cases!(|case_pred, case, case_succ| {
            check_div_round!(max, case);
            check_div_round!(case, max);
            check_div_round!(case, case);
            check_div_round!(case_pred, case);
            check_div_round!(case, case_pred);
            check_div_round!(case_succ, case);
            check_div_round!(case, case_succ);
        })
    }

    #[test, expected_failure(arithmetic_error, location = std::u256)]
    fun test_divide_and_round_up_error() {
        1u256.divide_and_round_up(0);
    }

    macro fun slow_pow($base: _, $exp: u8): _ {
        let base = $base;
        let mut exp = $exp;
        let mut result = 1;
        while (exp > 0) {
            result = result * base;
            exp = exp - 1;
        };
        result
    }

    #[test]
    fun test_pow() {
        cases!(|case_pred, case, case_succ| {
            assert_eq!(case_pred.pow(0), 1);
            assert_eq!(case_pred.pow(1), case_pred);
            assert_eq!(case.pow(0), 1);
            assert_eq!(case.pow(1),  case);
            assert_eq!(case_succ.pow(0), 1);
            assert_eq!(case_succ.pow(1), case_succ);
        });
        assert_eq!(0u256.pow(2), 0);
        assert_eq!(1u256.pow(255), 1);
        assert_eq!(2u256.pow(12), slow_pow!(2u256, 12));
        assert_eq!(3u256.pow(27), slow_pow!(3u256, 27));
    }

    #[test, expected_failure(arithmetic_error, location = std::u256)]
    fun test_pow_overflow() {
        255u256.pow(255);
    }
}
