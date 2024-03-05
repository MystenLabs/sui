module ml::ifixed_point32 {
    use std::fixed_point32::{create_from_rational, divide_u64, multiply_u64, get_raw_value, create_from_raw_value, FixedPoint32};

    struct IFixedPoint32 has copy, drop, store {
        value: FixedPoint32,
        sign: bool, // true when negative
    }

    public fun zero(): IFixedPoint32 {
        from_parts(create_from_raw_value(0), false)
    }

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

    public fun from_raw(value: u64, negative: bool): IFixedPoint32 {
        from_parts(create_from_raw_value(value), negative)
    }

    public fun from_rational(n: u64, d: u64, negative: bool): IFixedPoint32 {
        from_parts(create_from_rational(n, d), negative)
    }

    public fun multiply(a: IFixedPoint32, b: IFixedPoint32): IFixedPoint32 {
        let value = create_from_raw_value(multiply_u64(get_raw_value(a.value), b.value));
        let sign = a.sign != b.sign;
        from_parts(value, sign)
    }

    public fun multiply_with_constant(a: IFixedPoint32, n: u64): IFixedPoint32 {
        from_parts(create_from_raw_value(get_raw_value(a.value) * n), a.sign)
    }

    public fun divide(a: IFixedPoint32, b: IFixedPoint32): IFixedPoint32 {
        let value = create_from_raw_value(divide_u64(get_raw_value(a.value), b.value));
        let sign = a.sign != b.sign;
        from_parts(value, sign)
    }

    public fun divide_by_constant(a: IFixedPoint32, n: u64): IFixedPoint32 {
        from_parts(create_from_raw_value(get_raw_value(a.value) / n), a.sign)
    }

    public fun negate(a: IFixedPoint32): IFixedPoint32 {
        from_parts(a.value, !a.sign)
    }

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

    public fun is_zero(a: IFixedPoint32): bool {
        get_raw_value(a.value) == 0
    }

    public fun equals(a: IFixedPoint32, b: IFixedPoint32): bool {
        if (is_zero(a) && is_zero(b)) {
            return true
        };
        get_raw_value(a.value) == get_raw_value(b.value) && a.sign == b.sign
    }

    public fun is_negative(a: IFixedPoint32): bool {
        a.sign && !is_zero(a)
    }

    public fun raw_abs(a: IFixedPoint32): u64 {
        get_raw_value(a.value)
    }
}