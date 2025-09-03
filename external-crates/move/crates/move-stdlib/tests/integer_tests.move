// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// helpers for integer tests
#[test_only]
module std::integer_tests;

use std::unit_test::assert_eq;

public(package) macro fun cases($max: _, $cases: vector<_>, $f: |_, _, _|) {
    let mut cases = $cases;
    let max_pred = $max - 1;
    while (!cases.is_empty()) {
        let case = cases.pop_back();
        let case_pred = case.max(1) - 1;
        let case_succ = case.min(max_pred) + 1;
        $f(case_pred, case, case_succ);
    }
}

public(package) macro fun test_bitwise_not($max: _, $cases: vector<_>) {
    let max = $max;
    let cases = $cases;
    assert_eq!(max.bitwise_not(), 0);
    cases!(max, cases, |case_pred, case, case_succ| {
        assert_eq!(case_pred.bitwise_not().bitwise_not(), case_pred);
        assert_eq!(case_pred.bitwise_not() | case_pred, max);
        assert_eq!(case_pred.bitwise_not() ^ case_pred, max);
        assert_eq!(case_pred.bitwise_not() & case_pred, 0);

        assert_eq!(case.bitwise_not().bitwise_not(), case);
        assert_eq!(case.bitwise_not() | case, max);
        assert_eq!(case.bitwise_not() ^ case, max);
        assert_eq!(case.bitwise_not() & case, 0);

        assert_eq!(case_succ.bitwise_not().bitwise_not(), case_succ);
        assert_eq!(case_succ.bitwise_not() | case_succ, max);
        assert_eq!(case_succ.bitwise_not() ^ case_succ, max);
        assert_eq!(case_succ.bitwise_not() & case_succ, 0);
    })
}

public(package) macro fun test_max($max: _, $cases: vector<_>) {
    let max = $max;
    let cases = $cases;
    assert_eq!(max.max(max), max);
    cases!(max, cases, |case_pred, case, case_succ| {
        assert_eq!(max.max(case), max);
        assert_eq!(case.max(max), max);
        assert_eq!(case.max(case), case);
        assert_eq!(case_pred.max(case), case);
        assert_eq!(case_succ.max(case), case_succ);
    })
}

public(package) macro fun test_min($max: _, $cases: vector<_>) {
    let max = $max;
    let cases = $cases;
    assert_eq!(max.min(max), max);
    cases!(max, cases, |case_pred, case, case_succ| {
        assert_eq!(max.min(case), case);
        assert_eq!(case.min(max), case);
        assert_eq!(case.min(case), case);
        assert_eq!(case_pred.min(case), case_pred);
        assert_eq!(case_succ.min(case), case);
    })
}

public(package) macro fun test_diff($max: _, $cases: vector<_>) {
    let max = $max;
    let cases = $cases;
    assert_eq!(max.diff(max), 0);
    cases!(max, cases, |case_pred, case, case_succ| {
        assert_eq!(max.diff(case), max - case);
        assert_eq!(case.diff(max), max - case);
        assert_eq!(case.diff(case), 0);
        assert_eq!(case_pred.diff(case), case - case_pred);
        assert_eq!(case.diff(case_pred), case - case_pred);
        assert_eq!(case_succ.diff(case), case_succ - case);
        assert_eq!(case.diff(case_succ), case_succ - case);
    })
}

public(package) macro fun check_div_round($x: _, $y: _) {
    let x = $x;
    let y = $y;
    if (y == 0) return;
    assert_eq!(x.divide_and_round_up(y), (x / y) + (x % y).min(1));
}

public(package) macro fun test_divide_and_round_up($max: _, $cases: vector<_>) {
    let max = $max;
    let cases = $cases;
    assert_eq!(max.divide_and_round_up(max), 1);
    check_div_round!(max, max);
    cases!(max, cases, |case_pred, case, case_succ| {
        check_div_round!(max, case);
        check_div_round!(case, max);
        check_div_round!(case, case);
        check_div_round!(case_pred, case);
        check_div_round!(case, case_pred);
        check_div_round!(case_succ, case);
        check_div_round!(case, case_succ);
    })
}

public(package) macro fun slow_pow($base: _, $exp: u8): _ {
    let base = $base;
    let mut exp = $exp;
    let mut result = 1;
    while (exp > 0) {
        result = result * base;
        exp = exp - 1;
    };
    result
}

public(package) macro fun test_pow<$T>($max: $T, $cases: vector<$T>) {
    let max = $max;
    let cases = $cases;
    cases!(max, cases, |case_pred, case, case_succ| {
        assert_eq!(case_pred.pow(0), 1);
        assert_eq!(case_pred.pow(1), case_pred);
        assert_eq!(case.pow(0), 1);
        assert_eq!(case.pow(1), case);
        assert_eq!(case_succ.pow(0), 1);
        assert_eq!(case_succ.pow(1), case_succ);
    });
    assert_eq!((0: $T).pow(2), 0);
    assert_eq!((1: $T).pow(255), 1);
    assert_eq!((2: $T).pow(7), slow_pow!((2: $T), 7));
    assert_eq!((3: $T).pow(5), slow_pow!((3: $T), 5));
}

public(package) macro fun test_sqrt<$T>(
    $max: $T,
    $bound_cases: vector<$T>,
    $reflexive_cases: vector<$T>,
) {
    let max = $max;
    let cases = $bound_cases;
    // logical bounds cases
    let max_sqrt = max.sqrt();
    cases!(max, cases, |case_pred, case, case_succ| {
        let sqrt_pred = case_pred.sqrt();
        assert!(sqrt_pred * sqrt_pred <= case_pred);
        let sqrt = case.sqrt();
        assert!(sqrt * sqrt <= case);
        let sqrt_succ = case_succ.sqrt();
        assert!(sqrt_succ * sqrt_succ <= case_succ);

        if (sqrt_pred >= max_sqrt) return;
        assert!((sqrt_pred + 1) * (sqrt_pred + 1) > case_pred);

        if (sqrt >= max_sqrt) return;
        assert!((sqrt + 1) * (sqrt + 1) > case);

        if (sqrt_succ >= max_sqrt) return;
        assert!((sqrt_succ + 1) * (sqrt_succ + 1) > case_succ);
    });

    // simple reflexive cases
    let cases: vector<$T> = $reflexive_cases;
    cases!(max, cases, |case_pred, case, case_succ| {
        assert_eq!((case_pred * case_pred).sqrt(), case_pred);
        assert_eq!((case * case).sqrt(), case);
        assert_eq!((case_succ * case_succ).sqrt(), case_succ);
    });

    // test that the square of a non perfect square is the most recent square root perfect
    // square, rounding down
    let mut cases: vector<$T> = vector[2, 3, 4, 5, 6];
    while (!cases.is_empty()) {
        let case = cases.pop_back();
        let prev = case - 1;
        let square = case * case;
        let prev_suare = prev * prev;
        let mut i = prev_suare;
        while (i < square) {
            assert_eq!(i.sqrt(), prev);
            i = i + 1;
        }
    }
}

public(package) macro fun test_try_as_u8<$T>($max: $T) {
    assert_eq!((0: $T).try_as_u8(), option::some(0));
    assert_eq!((1: $T).try_as_u8(), option::some(1));
    assert_eq!((0xFF: $T).try_as_u8(), option::some(0xFF));
    assert_eq!((0xFF + 1: $T).try_as_u8(), option::none());
    let max = $max;
    assert_eq!(max.try_as_u8(), option::none());
}

public(package) macro fun test_try_as_u16<$T>($max: $T) {
    assert_eq!((0: $T).try_as_u16(), option::some(0));
    assert_eq!((1: $T).try_as_u16(), option::some(1));
    assert_eq!((0xFFFF: $T).try_as_u16(), option::some(0xFFFF));
    assert_eq!((0xFFFF + 1: $T).try_as_u16(), option::none());
    let max = $max;
    assert_eq!(max.try_as_u16(), option::none());
}

public(package) macro fun test_try_as_u32<$T>($max: $T) {
    assert_eq!((0: $T).try_as_u32(), option::some(0));
    assert_eq!((1: $T).try_as_u32(), option::some(1));
    assert_eq!((0xFFFF_FFFF: $T).try_as_u32(), option::some(0xFFFF_FFFF));
    assert_eq!((0xFFFF_FFFF + 1: $T).try_as_u32(), option::none());
    let max = $max;
    assert_eq!(max.try_as_u32(), option::none());
}

public(package) macro fun test_try_as_u64<$T>($max: $T) {
    assert_eq!((0: $T).try_as_u64(), option::some(0));
    assert_eq!((1: $T).try_as_u64(), option::some(1));
    assert_eq!((0xFFFF_FFFF_FFFF_FFFF: $T).try_as_u64(), option::some(0xFFFF_FFFF_FFFF_FFFF));
    assert_eq!((0xFFFF_FFFF_FFFF_FFFF + 1: $T).try_as_u64(), option::none());
    let max = $max;
    assert_eq!(max.try_as_u64(), option::none());
}

public(package) macro fun test_try_as_u128<$T>($max: $T) {
    assert_eq!((0: $T).try_as_u128(), option::some(0));
    assert_eq!((1: $T).try_as_u128(), option::some(1));
    assert_eq!(
        (0xFFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF: $T).try_as_u128(),
        option::some(0xFFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF),
    );
    assert_eq!((0xFFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF + 1: $T).try_as_u128(), option::none());
    let max = $max;
    assert_eq!(max.try_as_u128(), option::none());
}

public(package) macro fun sum_range<$T>($n: $T): $T {
    let n = $n;
    (n * (n + 1)) / 2
}

public(package) macro fun test_to_string<$T>() {
    assert_eq!((0: $T).to_string(), b"0".to_string());
    assert_eq!((1: $T).to_string(), b"1".to_string());
    assert_eq!((10: $T).to_string(), b"10".to_string());
    assert_eq!((11: $T).to_string(), b"11".to_string());
    assert_eq!((100: $T).to_string(), b"100".to_string());
    assert_eq!((111: $T).to_string(), b"111".to_string());
}

public(package) macro fun test_dos_case<$T>($case: $T) {
    let case = $case;
    let mut sum: $T = 0;
    case.do!(|i| sum = sum + i);
    assert_eq!(sum, sum_range!(case - 1));

    sum = 0;
    case.do_eq!(|i| sum = sum + i);
    assert_eq!(sum, sum_range!(case));

    let half = case / 2;

    sum = 0;
    half.range_do!(case, |i| sum = sum + i);
    assert_eq!(sum, sum_range!(case - 1) - sum_range!(half - 1));

    sum = 0;
    half.range_do_eq!(case, |i| sum = sum + i);
    assert_eq!(sum, sum_range!(case) - sum_range!(half - 1));
}

public(package) macro fun test_dos<$T>($max: $T, $cases: vector<$T>) {
    let max = $max;
    let cases = $cases;
    // test bounds/invalid ranges
    (0: $T).do!(|_| assert!(false));
    cases!(max, cases, |case_pred, case, case_succ| {
        if (case == 0) return;
        case.range_do!(0, |_| assert!(false));
        case.range_do_eq!(0, |_| assert!(false));

        if (case == max) return;
        case.range_do!(case_pred, |_| assert!(false));
        case_succ.range_do!(case, |_| assert!(false));
        case.range_do_eq!(case_pred, |_| assert!(false));
        case_succ.range_do_eq!(case, |_| assert!(false));
    });

    // test upper bound being max
    let max_pred = max - 1;
    max_pred.range_do_eq!(max, |_| ());

    // test iteration numbers
    let cases: vector<$T> = vector[3, 5, 8, 11, 14];
    cases!(max, cases, |case_pred, case, case_succ| {
        test_dos_case!(case_pred);
        test_dos_case!(case);
        test_dos_case!(case_succ);
    });
}
