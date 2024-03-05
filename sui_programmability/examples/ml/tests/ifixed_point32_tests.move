// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module ml::ifixed_point32_tests {
    use ml::ifixed_point32::{from_rational, add, divide, multiply, subtract, exp, from_raw};

    #[test]
    fun test_ifixed_point_arithmetic() {
        let a = from_rational(3, 1, false); // 3
        let b = from_rational(7, 2, true); // -3.5

        let expected_sum = from_rational(1, 2, true);
        let sum = add(a, b);
        assert!(sum == expected_sum, 0);

        let expected_product = from_rational(21, 2, true);
        let product = multiply(a, b);
        assert!(product == expected_product, 1);

        let expected_difference = from_rational(13, 2, false);
        let difference = subtract(a, b);
        assert!(difference == expected_difference, 2);

        let expected_quotient = from_rational(6, 7, true);
        let quotient = divide(a, b);
        assert!(quotient == expected_quotient, 3);

        let a = from_rational(3, 1, true); // -3
        let b = from_rational(7, 2, true); // -3.5

        let expected_sum = from_rational(13, 2, true);
        let sum = add(a, b);
        assert!(sum == expected_sum, 4);

        let expected_product = from_rational(21, 2, false);
        let product = multiply(a, b);
        assert!(product == expected_product, 5);

        let expected_difference = from_rational(1, 2, false);
        let difference = subtract(a, b);
        assert!(difference == expected_difference, 6);

        let expected_quotient = from_rational(6, 7, false);
        let quotient = divide(a, b);
        assert!(quotient == expected_quotient, 7);
    }

    #[test]
    fun test_exp() {
        let a = from_rational(5, 2, false); // 2.5
        let expected_exp = from_raw(52323414568, false);
        let exp = exp(a);
        assert!(exp == expected_exp, 0);
    }
}
