// exclude_for: cvc5
// separate_baseline: simplify
// (There used to be a separate baseline: cvc5, but now it's excluded)
// TODO(cvc5): cvc5 goes into infinite loop because of recursive function
// $pow (used in shifts) in prelude.bpl.
// TODO(cvc4): cvc4 currently produces false positives.
module 0x42::FixedPointArithmetic {

    use std::fixed_point32::{Self, FixedPoint32};
    spec module {
       pragma verify = true;
    }

    // -------------------------------
    // Multiplicative Property of Zero
    // -------------------------------

    fun multiply_0_x(x: FixedPoint32): u64 {
        fixed_point32::multiply_u64(0, x)
    }
    spec multiply_0_x {
        aborts_if false; // proved
        ensures result == 0; // proved
    }

    fun multiply_0_x_incorrect(x: FixedPoint32): u64 {
        fixed_point32::multiply_u64(0, x)
    }
    spec multiply_0_x_incorrect {
        aborts_if false; // proved
        ensures result == 1; // disproved
    }

    fun multiply_x_0(x: u64): u64 {
        fixed_point32::multiply_u64(x, fixed_point32::create_from_raw_value(0))
    }
    spec multiply_x_0 {
        aborts_if false; // proved
        ensures result == 0; // proved
    }

    fun multiply_x_0_incorrect(x: u64): u64 {
        fixed_point32::multiply_u64(x, fixed_point32::create_from_raw_value(0))
    }
    spec multiply_x_0_incorrect {
        aborts_if false; // proved
        ensures result == 1; // disproved
    }


    // -----------------------------------
    // Identity Property of Multiplication
    // -----------------------------------

    fun multiply_1_x(x: FixedPoint32): u64 {
        fixed_point32::multiply_u64(1, x)
    }
    spec multiply_1_x {
        aborts_if false; // proved
        // (x.value >> 32) is the integer part of x.
        ensures result == (x.value >> 32); // proved
    }

    fun multiply_1_x_incorrect(x: FixedPoint32): u64 {
        fixed_point32::multiply_u64(1, x)
    }
    spec multiply_1_x_incorrect {
        aborts_if false; // proved
        // (x.value >> 32) is the integer part of x.
        ensures result != (x.value >> 32); // disproved
    }

    fun multiply_x_1(x: u64): u64 {
        fixed_point32::multiply_u64(x, fixed_point32::create_from_rational(1,1))
    }
    spec multiply_x_1 {
        aborts_if false; // proved
        ensures result == x; // proved
    }

    fun multiply_x_1_incorrect(x: u64): u64 {
        fixed_point32::multiply_u64(x, fixed_point32::create_from_rational(1,1))
    }
    spec multiply_x_1_incorrect {
        aborts_if false; // proved
        ensures result != x; // disproved
    }


    // ---------------------------
    // Multiplication and Division
    // ---------------------------

    // Returns the evaluation of ((x * y) / y) in the fixed-point arithmetic
    fun mul_div(x: u64, y: FixedPoint32): u64 {
        let y_raw_val = fixed_point32::get_raw_value(y);
        let z = fixed_point32::multiply_u64(x, fixed_point32::create_from_raw_value(y_raw_val));
        fixed_point32::divide_u64(z, fixed_point32::create_from_raw_value(y_raw_val))
    }
    spec mul_div {
        ensures result <= x; // proved
    }

    fun mul_div_incorrect(x: u64, y: FixedPoint32): u64 {
        let y_raw_val = fixed_point32::get_raw_value(y);
        let z = fixed_point32::multiply_u64(x, fixed_point32::create_from_raw_value(y_raw_val));
        fixed_point32::divide_u64(z, fixed_point32::create_from_raw_value(y_raw_val))
    }
    spec mul_div_incorrect {
        ensures result >= x; // disproved
        //ensures result == x; // TODO: this can be disproved when given enought time, commented out because of timeout
        ensures result < x; // disproved
        ensures result > x; // disproved
    }

    // Returns the evaluation of ((x / y) * y) in the fixed-point arithmetic
    fun div_mul(x: u64, y: FixedPoint32): u64 {
        let y_raw_val = fixed_point32::get_raw_value(y);
        let z = fixed_point32::divide_u64(x, fixed_point32::create_from_raw_value(y_raw_val));
        fixed_point32::multiply_u64(z, fixed_point32::create_from_raw_value(y_raw_val))
    }
    spec div_mul {
        ensures result <= x; // proved
    }

    fun div_mul_incorrect(x: u64, y: FixedPoint32): u64 {
        let y_raw_val = fixed_point32::get_raw_value(y);
        let z = fixed_point32::divide_u64(x, fixed_point32::create_from_raw_value(y_raw_val));
        fixed_point32::multiply_u64(z, fixed_point32::create_from_raw_value(y_raw_val))
    }
    spec div_mul_incorrect {
        pragma verify=false; // TODO: disabled due to the CVC4 timeout
        ensures result >= x; // disproved
        ensures result == x; // disproved
        ensures result < x; // disproved
        ensures result > x; // disproved
    }

    fun mul_2_times_incorrect(a: u64, b: FixedPoint32, c: FixedPoint32): u64 {
        fixed_point32::multiply_u64(fixed_point32::multiply_u64(a, b), c)
    }
    spec mul_2_times_incorrect {
        // there exists a, b and c such that their product is equal to 10.
        ensures result != 10;
    }

    fun mul_3_times_incorrect(a: u64, b: FixedPoint32, c: FixedPoint32, d: FixedPoint32): u64 {
        fixed_point32::multiply_u64(fixed_point32::multiply_u64(fixed_point32::multiply_u64(a, b), c), d)
    }
    spec mul_3_times_incorrect {
        // there exists a, b, c and d such that their product is equal to 10.
        ensures result != 10;
    }
}
