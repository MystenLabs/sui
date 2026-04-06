//# init --edition development

// Casting to i8.
//# run
module 1::m {
fun main() {
    // 0 remains unchanged.
    assert!((0i8 as i8) == 0i8, 1000);
    assert!((0i16 as i8) == 0i8, 1001);
    assert!((0i32 as i8) == 0i8, 1002);
    assert!((0i64 as i8) == 0i8, 1003);
    assert!((0i128 as i8) == 0i8, 1004);
    assert!((0i256 as i8) == 0i8, 1005);

    // Small positive number unchanged.
    assert!((21i8 as i8) == 21i8, 1100);
    assert!((21i16 as i8) == 21i8, 1101);
    assert!((21i32 as i8) == 21i8, 1102);
    assert!((21i64 as i8) == 21i8, 1103);
    assert!((21i128 as i8) == 21i8, 1104);
    assert!((21i256 as i8) == 21i8, 1105);

    // Small negative number unchanged.
    assert!((-21i8 as i8) == -21i8, 1200);
    assert!((-21i16 as i8) == -21i8, 1201);
    assert!((-21i32 as i8) == -21i8, 1202);
    assert!((-21i64 as i8) == -21i8, 1203);
    assert!((-21i128 as i8) == -21i8, 1204);
    assert!((-21i256 as i8) == -21i8, 1205);

    // Max representable values.
    assert!((127i8 as i8) == 127i8, 1300);
    assert!((127i16 as i8) == 127i8, 1301);
    assert!((127i32 as i8) == 127i8, 1302);
    assert!((127i64 as i8) == 127i8, 1303);

    // Min representable values.
    assert!((-128i8 as i8) == -128i8, 1400);
    assert!((-128i16 as i8) == -128i8, 1401);
    assert!((-128i32 as i8) == -128i8, 1402);
    assert!((-128i64 as i8) == -128i8, 1403);
}
}

// Casting to i16.
//# run
module 2::m {
fun main() {
    assert!((0i8 as i16) == 0i16, 2000);
    assert!((0i16 as i16) == 0i16, 2001);
    assert!((0i64 as i16) == 0i16, 2002);

    // Widening: preserves negative.
    assert!((-128i8 as i16) == -128i16, 2100);
    assert!((127i8 as i16) == 127i16, 2101);

    // Max representable.
    assert!((32767i16 as i16) == 32767i16, 2200);
    assert!((32767i32 as i16) == 32767i16, 2201);
    assert!((-32768i16 as i16) == -32768i16, 2300);
    assert!((-32768i32 as i16) == -32768i16, 2301);
}
}

// Casting to i32.
//# run
module 3::m {
fun main() {
    assert!((0i8 as i32) == 0i32, 3000);
    assert!((-128i8 as i32) == -128i32, 3100);
    assert!((-32768i16 as i32) == -32768i32, 3101);
    assert!((2147483647i32 as i32) == 2147483647i32, 3200);
    assert!((-2147483648i32 as i32) == -2147483648i32, 3201);
    assert!((2147483647i64 as i32) == 2147483647i32, 3300);
    assert!((-2147483648i64 as i32) == -2147483648i32, 3301);
}
}

// Casting to i64.
//# run
module 4::m {
fun main() {
    assert!((0i8 as i64) == 0i64, 4000);
    assert!((-128i8 as i64) == -128i64, 4100);
    assert!((-32768i16 as i64) == -32768i64, 4101);
    assert!((-2147483648i32 as i64) == -2147483648i64, 4102);
    assert!((9223372036854775807i64 as i64) == 9223372036854775807i64, 4200);
    assert!((-9223372036854775808i64 as i64) == -9223372036854775808i64, 4201);
    assert!((9223372036854775807i128 as i64) == 9223372036854775807i64, 4300);
}
}

// Casting to i128.
//# run
module 5::m {
fun main() {
    assert!((0i8 as i128) == 0i128, 5000);
    assert!((-128i8 as i128) == -128i128, 5100);
    assert!((-9223372036854775808i64 as i128) == -9223372036854775808i128, 5101);
    assert!((170141183460469231731687303715884105727i128 as i128) == 170141183460469231731687303715884105727i128, 5200);
    assert!((-170141183460469231731687303715884105728i128 as i128) == -170141183460469231731687303715884105728i128, 5201);
}
}

// Casting to i256.
//# run
module 6::m {
fun main() {
    assert!((0i8 as i256) == 0i256, 6000);
    assert!((-128i8 as i256) == -128i256, 6100);
    assert!((-9223372036854775808i64 as i256) == -9223372036854775808i256, 6101);
    assert!((-170141183460469231731687303715884105728i128 as i256) == -170141183460469231731687303715884105728i256, 6102);
}
}

// Casting to i8, overflowing (positive).
//# run
module 7::m {
fun main() {
    // should fail
    (128i16 as i8);
}
}

//# run
module 8::m {
fun main() {
    // should fail
    (128i32 as i8);
}
}

//# run
module 9::m {
fun main() {
    // should fail
    (128i64 as i8);
}
}

// Casting to i8, overflowing (negative).
//# run
module 10::m {
fun main() {
    // should fail
    (-129i16 as i8);
}
}

//# run
module 11::m {
fun main() {
    // should fail
    (-129i32 as i8);
}
}

// Casting to i16, overflowing.
//# run
module 12::m {
fun main() {
    // should fail
    (32768i32 as i16);
}
}

//# run
module 13::m {
fun main() {
    // should fail
    (-32769i32 as i16);
}
}

//# run
module 14::m {
fun main() {
    // should fail
    (32768i64 as i16);
}
}

// Casting to i32, overflowing.
//# run
module 15::m {
fun main() {
    // should fail
    (2147483648i64 as i32);
}
}

//# run
module 16::m {
fun main() {
    // should fail
    (-2147483649i64 as i32);
}
}

// Casting to i64, overflowing.
//# run
module 17::m {
fun main() {
    // should fail
    (9223372036854775808i128 as i64);
}
}

//# run
module 18::m {
fun main() {
    // should fail
    (-9223372036854775809i128 as i64);
}
}

// Casting to i128, overflowing.
//# run
module 19::m {
fun main() {
    // should fail
    (170141183460469231731687303715884105728i256 as i128);
}
}

//# run
module 20::m {
fun main() {
    // should fail
    (-170141183460469231731687303715884105729i256 as i128);
}
}
