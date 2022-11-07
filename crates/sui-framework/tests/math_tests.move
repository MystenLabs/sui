// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::math_tests {
    use sui::math;

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
}
