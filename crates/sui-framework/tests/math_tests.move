// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::math_tests {
    use sui::math;
    use sui::math::sqrt_u256;

    #[test]
    fun test_max() {
        assert!(math::max(10, 100) == 100, 1);
        assert!(math::max(100, 10) == 100, 2);
        assert!(math::max(0, 0) == 0, 3);
    }

    #[test]
    fun test_min() {
        assert!(math::min(10, 100) == 10, 1);
        assert!(math::min(100, 10) == 10, 2);
        assert!(math::min(0, 0) == 0, 3);
    }

    #[test]
    fun test_pow() {
        assert!(math::pow(1, 0) == 1, 0);
        assert!(math::pow(3, 1) == 3, 0);
        assert!(math::pow(2, 10) == 1024, 0);
        assert!(math::pow(10, 6) == 1000000, 0);
    }

    #[test]
    #[expected_failure]
    fun test_pow_overflow() {
        math::pow(10, 100);
    }

    #[test]
    fun test_perfect_sqrt() {
        let i = 0;
        while (i < 1000) {
            assert!(math::sqrt(i * i) == i, 1);
            i = i + 1;
        };
        let i = 0xFFFFFFFFFu128;
        while (i < 0xFFFFFFFFFu128 + 1) {
            assert!(math::sqrt_u128(i * i) == i, 1);
            i = i + 1;
        }
    }

    #[test]
    // This function tests whether the (square root)^2 equals the
    // initial value OR whether it is equal to the nearest lower
    // number that does.
    fun test_imperfect_sqrt() {
        let i = 1;
        let prev = 1;
        while (i <= 1000) {
            let root = math::sqrt(i);

            assert!(i == root * root || root == prev, 0);

            prev = root;
            i = i + 1;
        }
    }

    #[test]
    fun test_sqrt_big_numbers() {
        let u64_max = 18446744073709551615;
        assert!(4294967295 == math::sqrt(u64_max), 0)
    }

    #[test]
    fun test_sqrt_u256() {
        let i = 0;
        while (i <= 5) {
            assert!(i == sqrt_u256(i * i), 0);
            i = i + 1;
        };

        // python3
        // import math
        // int(math.sqrt(115792089237316195423570985008687907853269984665640564039457584007913129639935))
        // result: 340282366920938463463374607431768211456
        // different here is 1 which is acceptable
        let u256_max = 115792089237316195423570985008687907853269984665640564039457584007913129639935;
        assert!(340282366920938463463374607431768211455 == math::sqrt_u256(u256_max), 0);
    }
}
