//# init --edition development

//# publish
module 0x42::signed_fns {

    // Simple function taking and returning i8
    public fun negate_i8(x: i8): i8 {
        -x
    }

    // Function taking multiple signed params of different types
    public fun mixed_signed(a: i8, b: i16, c: i32, d: i64): i128 {
        (a as i128) + (b as i128) + (c as i128) + (d as i128)
    }

    // Recursive countdown from positive toward negative
    public fun countdown(n: i32): i32 {
        if (n <= -3i32) {
            n
        } else {
            countdown(n - 1)
        }
    }

    // Function returning negative values
    public fun negative_value(): i64 {
        -999i64
    }

    // Helper for composition: doubles a signed value
    public fun double_i64(x: i64): i64 {
        x * 2
    }

    // Compose: pass signed int result of one function to another
    public fun double_negative(): i64 {
        double_i64(negative_value())
    }

    public fun test_negate() {
        assert!(negate_i8(5i8) == -5i8, 0);
        assert!(negate_i8(-10i8) == 10i8, 1);
        assert!(negate_i8(0i8) == 0i8, 2);
    }

    public fun test_mixed() {
        let result = mixed_signed(1i8, 2i16, 3i32, 4i64);
        assert!(result == 10i128, 0);

        let result2 = mixed_signed(-1i8, -2i16, -3i32, -4i64);
        assert!(result2 == -10i128, 1);
    }

    public fun test_countdown() {
        assert!(countdown(2i32) == -3i32, 0);
        assert!(countdown(0i32) == -3i32, 1);
        assert!(countdown(-3i32) == -3i32, 2);
        assert!(countdown(-10i32) == -10i32, 3);
    }

    public fun test_negative_return() {
        assert!(negative_value() == -999i64, 0);
    }

    public fun test_composition() {
        assert!(double_negative() == -1998i64, 0);
        assert!(double_i64(50i64) == 100i64, 1);
        assert!(double_i64(-7i64) == -14i64, 2);
    }
}

//# run 0x42::signed_fns::test_negate

//# run 0x42::signed_fns::test_mixed

//# run 0x42::signed_fns::test_countdown

//# run 0x42::signed_fns::test_negative_return

//# run 0x42::signed_fns::test_composition
