module ml::ifixed_point32 {
    use std::fixed_point32::{create_from_rational, divide_u64, multiply_u64, get_raw_value, create_from_raw_value, FixedPoint32};

    /// Represents a signed fixed point number with 32 bits fractional part and 32 bits integer part.
    struct IFixedPoint32 has copy, drop, store {
        value: FixedPoint32,
        sign: bool, // true when negative
    }

    /// Return a representation of 1.
    public fun one(): IFixedPoint32 {
        from_integer(1, false)
    }

    /// Return a representation of 0.
    public fun zero(): IFixedPoint32 {
        from_parts(create_from_raw_value(0), false)
    }

    /// Create a signed fixed point number from an unsigned number and a sign bit.
    public fun from_parts(value: FixedPoint32, is_negative: bool): IFixedPoint32 {
        let sign = if (std::fixed_point32::is_zero(value)) {
            false
        } else {
            is_negative
        };

        IFixedPoint32 {
            value: value,
            sign: sign
        }
    }

    /// Create a signed fixed point number from the raw integer representstion. 
    /// This will be a representation of `value / 2^32`.
    public fun from_raw(value: u64, negative: bool): IFixedPoint32 {
        from_parts(create_from_raw_value(value), negative)
    }

    /// Create an (potentially approximate) representation of a rational number `n/d`.
    public fun from_rational(n: u64, d: u64, negative: bool): IFixedPoint32 {
        from_parts(create_from_rational(n, d), negative)
    }

    /// Create a representation of an integer `n`
    public fun from_integer(n: u64, negative: bool): IFixedPoint32 {
        from_parts(create_from_raw_value(n << 32), negative)
    }

    /// Multiply two signed fixed point numbers.
    public fun multiply(a: IFixedPoint32, b: IFixedPoint32): IFixedPoint32 {
        let value = create_from_raw_value(multiply_u64(get_raw_value(a.value), b.value));
        let sign = a.sign != b.sign;
        from_parts(value, sign)
    }

    /// Multiply a signed fixed point number `a` with an integer `n`.
    public fun multiply_with_constant(a: IFixedPoint32, n: u64): IFixedPoint32 {
        from_parts(create_from_raw_value(get_raw_value(a.value) * n), a.sign)
    }

    /// Divide two fixed point numbers.
    public fun divide(a: IFixedPoint32, b: IFixedPoint32): IFixedPoint32 {
        let value = create_from_raw_value(divide_u64(get_raw_value(a.value), b.value));
        let sign = a.sign != b.sign;
        from_parts(value, sign)
    }

    /// Divide a signed fixed point number `a` by a constant `n`.
    public fun divide_by_constant(a: IFixedPoint32, n: u64): IFixedPoint32 {
        from_parts(create_from_raw_value(get_raw_value(a.value) / n), a.sign)
    }

    /// Return `-a`
    public fun negate(a: IFixedPoint32): IFixedPoint32 {
        from_parts(a.value, !a.sign)
    }

    /// Return `a + b`.
    public fun add(a: IFixedPoint32, b: IFixedPoint32): IFixedPoint32 {
        if (is_zero(a)) {
            return b
        } else if (is_zero(b)) {
            return a
        };

        if (a.sign != b.sign) {
            return subtract(a, negate(b))
        };

        let value = create_from_raw_value(get_raw_value(a.value) + get_raw_value(b.value));
        let sign = a.sign;

        from_parts(value, sign)
    }

    /// Compute `a - b`.
    public fun subtract(a: IFixedPoint32, b: IFixedPoint32): IFixedPoint32 {
        if (is_zero(a)) {
            return negate(b)
        } else if (is_zero(b)) {
            return a
        };

        if (a.sign != b.sign) {
            return add(a, negate(b))
        };

        // Inputs have same sign now
        let sign = a.sign;

        let a_raw = get_raw_value(a.value);
        let b_raw = get_raw_value(b.value);
        let difference = if (a_raw >= b_raw) {
            a_raw - b_raw
        } else {
            sign = !sign;
            b_raw - a_raw
        };

        from_parts(create_from_raw_value(difference), sign)
    }

    /// Test if `a == 0`.
    public fun is_zero(a: IFixedPoint32): bool {
        get_raw_value(a.value) == 0
    }

    /// Test if `a < 0`.
    public fun is_negative(a: IFixedPoint32): bool {
        a.sign && !is_zero(a)
    }

    /// Get the integer representation of `|a|`.
    public fun raw_abs(a: IFixedPoint32): u64 {
        get_raw_value(a.value)
    }

    /// Get `floor(a)` for an unsigned `a`.
    public fun integer_part(a: FixedPoint32): u64 {
        get_raw_value(a) >> 32
    }

    /// Get `a - floor(a)` for an unsigned `a`.
    public fun fractional_part(a: FixedPoint32): FixedPoint32 {
        create_from_raw_value(get_raw_value(a) & 0xFFFFFFFF)
    }

    /// Evaluate a polynomial `p` represented by its raw coefficients on a point `x`
    fun polynomial_evaluation(x: IFixedPoint32, p: vector<u64>): IFixedPoint32 {
        let result: IFixedPoint32 = from_raw(*std::vector::borrow(&p, 0), false);
        let i = 1;
        let xi = x;
        let length = std::vector::length(&p);
        while (i < length) {
            let ci = from_raw(*std::vector::borrow(&p, i), false);
            result = add(result, multiply(xi, ci));
            i = i + 1;
            if (i < length) {
                xi = multiply(xi, x);
            };
        };
        result
    }

    // The raw part of an approximation of log_2(e)
    const LOG2_E: u64 = 6196328018;

    // A polynomial approximation of the exponential function on [0,1]
    const P: vector<u64> = vector[4294967628, 2977044471, 1031765007, 238388159, 41310461, 5724033, 666181, 60979];

    /// Compute `exp(x)`.
    public fun exp(x: IFixedPoint32): IFixedPoint32 {

        // Compute exp(x) as  2^{y} = 2^{ipart(y)) * 2^{fpart(y)) where y = |log2(e) * x|
        // and take reciprocal if x is negative.

        let y = multiply(x, from_raw(LOG2_E, false));
        let integer_part = integer_part(y.value);
        let fractional_part = from_parts(fractional_part(y.value), false);

        let f = 1 << (integer_part as u8);
        let g = polynomial_evaluation(fractional_part, P);
        let h = multiply_with_constant(g, f);

        if (x.sign) {
            divide(one(), h)
        } else {
            h
        }
    }
}