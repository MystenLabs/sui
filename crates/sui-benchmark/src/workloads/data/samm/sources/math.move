// Copyright 2022 OmniBTC Authors. Licensed under Apache-2.0 License.
module swap::math {

    const ERR_DIVIDE_BY_ZERO: u64 = 500;
    const ERR_U64_OVERFLOW: u64 = 501;

    const U64_MAX: u64 = 18446744073709551615;

    /// Multiple two u64 and get u128, e.g. ((`x` * `y`) as u128).
    public fun mul_to_u128(x: u64, y: u64): u128 {
        (x as u128) * (y as u128)
    }

    /// Get square root of `y`.
    /// Babylonian method (https://en.wikipedia.org/wiki/Methods_of_computing_square_roots#Babylonian_method)
    public fun sqrt(y: u128): u64 {
        if (y < 4) {
            if (y == 0) {
                0u64
            } else {
                1u64
            }
        } else {
            let z = y;
            let x = y / 2 + 1;
            while (x < z) {
                z = x;
                x = (y / x + x) / 2;
            };
            (z as u64)
        }
    }

    public fun sqrt_64(y: u64): u64 {
        if (y < 4) {
            if (y == 0) {
                0u64
            } else {
                1u64
            }
        } else {
            let z = y;
            let x = y / 2 + 1;
            while (x < z) {
                z = x;
                x = (y / x + x) / 2;
            };
            (z as u64)
        }
    }

    /// Implements: `x` * `y` / `z`.
    public fun mul_div(
        x: u64,
        y: u64,
        z: u64
    ): u64 {
        assert!(z != 0, ERR_DIVIDE_BY_ZERO);
        let r = (x as u128) * (y as u128) / (z as u128);
        assert!(!(r > (U64_MAX as u128)), ERR_U64_OVERFLOW);
        (r as u64)
    }

    /// Implements: `x` * `y` / `z`.
    public fun mul_div_u128(
        x: u128,
        y: u128,
        z: u128
    ): u64 {
        assert!(z != 0, ERR_DIVIDE_BY_ZERO);
        let r = x * y / z;
        assert!(!(r > (U64_MAX as u128)), ERR_U64_OVERFLOW);
        (r as u64)
    }
}
