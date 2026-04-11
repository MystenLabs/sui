//# init --edition development

// Decimal literals for all signed types
//# run
module 1::m {
fun main() {
    // i8
    assert!(0i8 == 0i8, 100);
    assert!(1i8 == 1i8, 101);
    assert!(42i8 == 42i8, 102);
    assert!(127i8 == 127i8, 103);
    assert!(-1i8 == -1i8, 104);
    assert!(-128i8 == -128i8, 105);

    // i16
    assert!(0i16 == 0i16, 200);
    assert!(1i16 == 1i16, 201);
    assert!(1000i16 == 1000i16, 202);
    assert!(32767i16 == 32767i16, 203);
    assert!(-1i16 == -1i16, 204);
    assert!(-32768i16 == -32768i16, 205);

    // i32
    assert!(0i32 == 0i32, 300);
    assert!(1i32 == 1i32, 301);
    assert!(42i32 == 42i32, 302);
    assert!(123456i32 == 123456i32, 303);
    assert!(2147483647i32 == 2147483647i32, 304);
    assert!(-1i32 == -1i32, 305);
    assert!(-2147483648i32 == -2147483648i32, 306);

    // i64
    assert!(0i64 == 0i64, 400);
    assert!(1i64 == 1i64, 401);
    assert!(123456i64 == 123456i64, 402);
    assert!(9223372036854775807i64 == 9223372036854775807i64, 403);
    assert!(-1i64 == -1i64, 404);
    assert!(-9223372036854775808i64 == -9223372036854775808i64, 405);

    // i128
    assert!(0i128 == 0i128, 500);
    assert!(1i128 == 1i128, 501);
    assert!(170141183460469231731687303715884105727i128 == 170141183460469231731687303715884105727i128, 502);
    assert!(-1i128 == -1i128, 503);
    assert!(-170141183460469231731687303715884105728i128 == -170141183460469231731687303715884105728i128, 504);

    // i256
    assert!(0i256 == 0i256, 600);
    assert!(1i256 == 1i256, 601);
    assert!(
        57896044618658097711785492504343953926634992332820282019728792003956564819967i256
            == 57896044618658097711785492504343953926634992332820282019728792003956564819967i256,
        602,
    );
    assert!(-1i256 == -1i256, 603);
    assert!(
        -57896044618658097711785492504343953926634992332820282019728792003956564819968i256
            == -57896044618658097711785492504343953926634992332820282019728792003956564819968i256,
        604,
    );
}
}

// Underscore separators in signed literals
//# run
module 2::m {
fun main() {
    assert!(1_000i32 == 1000i32, 700);
    assert!(-1_000i64 == -1000i64, 701);
    assert!(1_000_000i64 == 1000000i64, 702);
    assert!(1__000i32 == 1000i32, 703);
    assert!(1_000___i32 == 1000i32, 704);
    assert!(-1_000_000i128 == -1000000i128, 705);
    assert!(12_34_56i32 == 123456i32, 706);
}
}

// Signed literals in expressions and assignments
//# run
module 3::m {
fun main() {
    // Assignment
    let x: i8 = 42i8;
    assert!(x == 42i8, 800);

    let y: i8 = -42i8;
    assert!(y == -42i8, 801);

    // In expressions
    let z: i32 = 10i32 + 20i32;
    assert!(z == 30i32, 802);

    let w: i32 = -10i32 * 3i32;
    assert!(w == -30i32, 803);

    // Mixed with comparisons
    assert!(1i8 > -1i8, 807);
    assert!(-128i8 < 127i8, 808);
    assert!(0i16 >= -1i16, 809);
    assert!(-1i32 <= 0i32, 810);
}
}

// Signed literals as function arguments
//# publish
module 5::helpers {
public fun add(a: i32, b: i32): i32 { a + b }
public fun negate(x: i64): i64 { -x }
}

//# run
module 6::m {
use 5::helpers;
fun main() {
    assert!(helpers::add(10i32, -20i32) == -10i32, 804);
    assert!(helpers::negate(42i64) == -42i64, 805);
    assert!(helpers::negate(-100i64) == 100i64, 806);
}
}

// Negative literal edge cases
//# run
module 4::m {
fun main() {
    // Negative zero
    assert!(-0i8 == 0i8, 900);
    assert!(-0i16 == 0i16, 901);
    assert!(-0i32 == 0i32, 902);
    assert!(-0i64 == 0i64, 903);
    assert!(-0i128 == 0i128, 904);
    assert!(-0i256 == 0i256, 905);

    // Negative of negative (via variable, not double-negate literal)
    let x: i8 = -42i8;
    let y: i8 = -x;
    assert!(y == 42i8, 906);
}
}
