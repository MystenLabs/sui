// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module Sui::MathTests {
    use Sui::Math;

    #[test]
    fun test_max() {
        assert!(Math::max(10, 100) == 100, 0);
        assert!(Math::max(100, 10) == 100, 0);
        assert!(Math::max(0, 0) == 0, 0);
    }

    #[test]
    fun test_min() {
        assert!(Math::min(10, 100) == 10, 0);
        assert!(Math::min(100, 10) == 10, 0);
        assert!(Math::min(0, 0) == 0, 0);
    }

    #[test]
    fun test_sqrt() {
        let i = 0;
        while (i < 1000) {
            assert!(Math::sqrt(i * i) == i, 0);
            i = i + 1;
        }
    }
}
