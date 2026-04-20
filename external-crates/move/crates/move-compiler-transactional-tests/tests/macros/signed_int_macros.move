//# init --edition development

//# publish

module 0x42::m {

    macro fun negate($x: i64): i64 {
        -$x
    }

    macro fun abs($x: i64): i64 {
        let v = $x;
        if (v < 0i64) -v else v
    }

    macro fun clamp($x: i64, $lo: i64, $hi: i64): i64 {
        let v = $x;
        let lo = $lo;
        let hi = $hi;
        if (v < lo) lo
        else if (v > hi) hi
        else v
    }

    macro fun literal_in_body(): i64 {
        -42i64
    }

    macro fun apply($x: i64, $f: |i64| -> i64): i64 {
        $f($x)
    }

    entry fun test_negate() {
        assert!(negate!(10i64) == -10i64, 0);
        assert!(negate!(-5i64) == 5i64, 1);
        assert!(negate!(0i64) == 0i64, 2);
    }

    entry fun test_abs() {
        assert!(abs!(-100i64) == 100i64, 0);
        assert!(abs!(100i64) == 100i64, 1);
        assert!(abs!(0i64) == 0i64, 2);
    }

    entry fun test_clamp() {
        assert!(clamp!(50i64, -10i64, 10i64) == 10i64, 0);
        assert!(clamp!(-50i64, -10i64, 10i64) == -10i64, 1);
        assert!(clamp!(5i64, -10i64, 10i64) == 5i64, 2);
    }

    entry fun test_literal_in_body() {
        assert!(literal_in_body!() == -42i64, 0);
    }

    entry fun test_lambda_capture() {
        let captured: i64 = -7i64;
        let result = apply!(3i64, |x| x + captured);
        assert!(result == -4i64, 0);
    }
}

//# run 0x42::m::test_negate

//# run 0x42::m::test_abs

//# run 0x42::m::test_clamp

//# run 0x42::m::test_literal_in_body

//# run 0x42::m::test_lambda_capture
