//# init --edition development

// Bitwise AND
//# run
module 1::m {
fun main() {
    // Zero AND anything = zero
    assert!(0i8 & 0i8 == 0i8, 1000);
    assert!(0i16 & 0i16 == 0i16, 1001);
    assert!(0i32 & 0i32 == 0i32, 1002);
    assert!(0i64 & 0i64 == 0i64, 1003);
    assert!(0i128 & 0i128 == 0i128, 1004);
    assert!(0i256 & 0i256 == 0i256, 1005);

    assert!(0i8 & 42i8 == 0i8, 1010);
    assert!(0i16 & 42i16 == 0i16, 1011);
    assert!(0i32 & 42i32 == 0i32, 1012);
    assert!(0i64 & 42i64 == 0i64, 1013);
    assert!(0i128 & 42i128 == 0i128, 1014);
    assert!(0i256 & 42i256 == 0i256, 1015);

    // x AND x = x
    assert!(43i8 & 43i8 == 43i8, 1100);
    assert!(43i16 & 43i16 == 43i16, 1101);
    assert!(43i32 & 43i32 == 43i32, 1102);
    assert!(43i64 & 43i64 == 43i64, 1103);
    assert!(43i128 & 43i128 == 43i128, 1104);
    assert!(43i256 & 43i256 == 43i256, 1105);

    // Negative values
    assert!(-1i8 & -1i8 == -1i8, 1200);
    assert!(-1i8 & 127i8 == 127i8, 1201);
    assert!(-128i8 & -1i8 == -128i8, 1202);
    assert!(-128i8 & 127i8 == 0i8, 1203);

    // Mixed operations
    assert!(101i8 & 77i8 == 69i8, 1300);
}
}

// Bitwise OR
//# run
module 2::m {
fun main() {
    // Zero OR anything = anything
    assert!(0i8 | 0i8 == 0i8, 2000);
    assert!(42i8 | 0i8 == 42i8, 2001);
    assert!(0i16 | 42i16 == 42i16, 2002);
    assert!(0i32 | 42i32 == 42i32, 2003);
    assert!(0i64 | 42i64 == 42i64, 2004);
    assert!(0i128 | 42i128 == 42i128, 2005);
    assert!(0i256 | 42i256 == 42i256, 2006);

    // x OR x = x
    assert!(43i8 | 43i8 == 43i8, 2100);
    assert!(43i16 | 43i16 == 43i16, 2101);

    // Negative values
    assert!(-1i8 | 0i8 == -1i8, 2200);
    assert!(-128i8 | 127i8 == -1i8, 2201);

    // Mixed operations
    assert!(101i8 | 77i8 == 109i8, 2300);
}
}

// Bitwise XOR
//# run
module 3::m {
fun main() {
    // Zero XOR anything = anything
    assert!(0i8 ^ 0i8 == 0i8, 3000);
    assert!(13i8 ^ 0i8 == 13i8, 3001);
    assert!(0i16 ^ 13i16 == 13i16, 3002);
    assert!(0i32 ^ 13i32 == 13i32, 3003);
    assert!(0i64 ^ 13i64 == 13i64, 3004);
    assert!(0i128 ^ 13i128 == 13i128, 3005);
    assert!(0i256 ^ 13i256 == 13i256, 3006);

    // x XOR x = 0
    assert!(43i8 ^ 43i8 == 0i8, 3100);
    assert!(43i16 ^ 43i16 == 0i16, 3101);
    assert!(43i32 ^ 43i32 == 0i32, 3102);
    assert!(43i64 ^ 43i64 == 0i64, 3103);
    assert!(43i128 ^ 43i128 == 0i128, 3104);
    assert!(43i256 ^ 43i256 == 0i256, 3105);

    // Negative values
    assert!(-1i8 ^ -1i8 == 0i8, 3200);
    assert!(-128i8 ^ -128i8 == 0i8, 3201);
    assert!(-1i8 ^ 0i8 == -1i8, 3202);

    // Mixed operations
    assert!(101i8 ^ 77i8 == 40i8, 3300);
    assert!(13i8 ^ 1i8 == 12i8, 3301);
}
}
