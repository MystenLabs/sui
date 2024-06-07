// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module std::u32_tests {
    use std::integer_tests;
    use std::unit_test::assert_eq;

    const BIT_SIZE: u8 = 32;
    const MAX: u32 = 0xFFFF_FFFF;
    const MAX_PRED: u32 = MAX - 1;

    const CASES: vector<u32> = vector[
        0,
        1,
        10,
        11,
        100,
        111,
        1 << (BIT_SIZE / 2 - 1),
        (1 << (BIT_SIZE / 2 - 1)) + 1,
        1 << (BIT_SIZE - 1),
        (1 << (BIT_SIZE - 1)) + 1,
        MAX / 2,
        (MAX / 2) + 1,
        MAX_PRED,
        MAX,
    ];

    #[test]
    fun test_max() {
        integer_tests::test_max!(MAX, CASES);
    }

    #[test]
    fun test_min() {
        integer_tests::test_min!(MAX, CASES);
    }

    #[test]
    fun test_diff() {
        integer_tests::test_diff!(MAX, CASES);
    }

    #[test]
    fun test_divide_and_round_up() {
        integer_tests::test_divide_and_round_up!(MAX, CASES);
    }

    #[test, expected_failure(arithmetic_error, location = std::u8)]
    fun test_divide_and_round_up_error() {
        1u8.divide_and_round_up(0);
    }

    #[test]
    fun test_pow() {
        integer_tests::test_pow!(MAX, CASES);
        assert_eq!(2u32.pow(12), integer_tests::slow_pow!(2u32, 12));
        assert_eq!(3u32.pow(20), integer_tests::slow_pow!(3u32, 20));
    }

    #[test, expected_failure(arithmetic_error, location = std::u32)]
    fun test_pow_overflow() {
        255u32.pow(255);
    }

    #[test]
    fun test_sqrt() {
        let reflexive_cases =
            vector[0, 2, 5, 8, 11, 14, 17, 20, 23, 26, 29, 32, 35, 38, 41, 44, 47, 50, 53, 56, 59];
        integer_tests::test_sqrt!(MAX, CASES, reflexive_cases)
    }

    fun sum_range(n: u32): u32 {
        (n * (n + 1)) / 2
    }

    fun test_dos_case(case: u32) {
        let mut sum = 0;
        case.do!(|i| sum = sum + i);
        assert_eq!(sum, sum_range(case - 1));

        sum = 0;
        case.do_eq!(|i| sum = sum + i);
        assert_eq!(sum, sum_range(case));

        let half = case / 2;

        sum = 0;
        half.range_do!(case, |i| sum = sum + i);
        assert_eq!(sum, sum_range(case - 1) - sum_range(half - 1));

        sum = 0;
        half.range_do_eq!(case, |i| sum = sum + i);
        assert_eq!(sum, sum_range(case) - sum_range(half - 1));
    }

    #[test]
    fun test_dos() {
        // test bounds/invalid ranges
        0u32.do!(|_| assert!(false));
        cases!(|case_pred, case, case_succ| {
            if (case == 0) return;
            case.range_do!(0, |_| assert!(false));
            case.range_do_eq!(0, |_| assert!(false));

            if (case == MAX) return;
            case.range_do!(case_pred, |_| assert!(false));
            case_succ.range_do!(case, |_| assert!(false));
            case.range_do_eq!(case_pred, |_| assert!(false));
            case_succ.range_do_eq!(case, |_| assert!(false));
        });

        // test iteration numbers
        let cases: vector<u32> = vector[4, 42, 112, 255];
        custom_cases!(cases, |case_pred, case, case_succ| {
            test_dos_case(case_pred);
            test_dos_case(case);
            test_dos_case(case_succ);
        });
    }
}
