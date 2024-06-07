// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module std::u8_tests {
    use std::integer_tests;

    const BIT_SIZE: u8 = 8;
    const MAX: u8 = 0xFF;
    const MAX_PRED: u8 = MAX - 1;

    const CASES: vector<u8> = vector[
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
    }

    #[test, expected_failure(arithmetic_error, location = std::u8)]
    fun test_pow_overflow() {
        255u8.pow(255);
    }

    #[test]
    fun test_sqrt() {
        integer_tests::test_sqrt!(MAX, CASES, vector[0, 2, 5, 8, 11, 14]);
    }

    fun sum_range(n: u8): u8 {
        (n * (n + 1)) / 2
    }

    fun test_dos_case(case: u8) {
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
        0u8.do!(|_| assert!(false));
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
        let cases: vector<u8> = vector[4, 7, 10, 13];
        custom_cases!(cases, |case_pred, case, case_succ| {
            test_dos_case(case_pred);
            test_dos_case(case);
            test_dos_case(case_succ);
        });
    }
}
