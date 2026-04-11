//# init --edition development

//# publish
module 0x42::m {
    public fun aborter_i8(x: u64): i8 {
        abort x
    }

    public fun aborter_i64(x: u64): i64 {
        abort x
    }

    public fun aborter_i128(x: u64): i128 {
        abort x
    }
}

// Left operand aborts before addition is evaluated
//# run
module 0x43::test0 {
    // abort with 1
    public fun main(): i8 {
        0x42::m::aborter_i8(1) + 2i8
    }
}

// Left operand aborts before right operand
//# run
module 0x44::test1 {
    // abort with 1
    public fun main(): i64 {
        0x42::m::aborter_i64(1) + 0x42::m::aborter_i64(2)
    }
}

// Left operand overflow aborts before right side is evaluated
//# run
module 0x45::test2 {
    // aborts with bad math (i8 overflow: 127 + 1)
    public fun main(): i8 {
        {127i8 + 1i8} + {abort 55; 5i8}
    }
}

// Division where left operand aborts
//# run
module 0x46::test3 {
    // abort with 10
    public fun main(): i64 {
        0x42::m::aborter_i64(10) / 0i64
    }
}

// Division by zero on right side (left evaluates first and succeeds)
//# run
module 0x47::test4 {
    // aborts with bad math (division by zero)
    public fun main(): i64 {
        42i64 / 0i64
    }
}

// Left abort in function args evaluated left to right
//# run
module 0x48::test5 {
    // abort with 0
    public fun main(): i128 {
        (abort 0) + {(abort 14); 0i128} + 0i128
    }
}

// Subtraction underflow aborts
//# run
module 0x49::test6 {
    // aborts with bad math (i8 underflow: -128 - 1)
    public fun main(): i8 {
        {-128i8 - 1i8} + {abort 99; 0i8}
    }
}
