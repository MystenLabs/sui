//# init --edition development

// Shift count >= bit width aborts.
//# run
module 1::m {
fun main() {
    // should fail
    0i8 << 8u8;
}
}

//# run
module 2::m {
fun main() {
    // should fail
    0i16 << 16u8;
}
}

//# run
module 3::m {
fun main() {
    // should fail
    0i32 << 32u8;
}
}

//# run
module 4::m {
fun main() {
    // should fail
    0i64 << 64u8;
}
}

//# run
module 5::m {
fun main() {
    // should fail
    0i128 << 128u8;
}
}

//# run
module 6::m {
fun main() {
    // should fail
    0i8 >> 8u8;
}
}

//# run
module 7::m {
fun main() {
    // should fail
    0i16 >> 16u8;
}
}

//# run
module 8::m {
fun main() {
    // should fail
    0i32 >> 32u8;
}
}

//# run
module 9::m {
fun main() {
    // should fail
    0i64 >> 64u8;
}
}

//# run
module 10::m {
fun main() {
    // should fail
    0i128 >> 128u8;
}
}

// Shifting 0 results in 0.
//# run
module 11::m {
fun main() {
    assert!(0i8 << 4u8 == 0i8, 1000);
    assert!(0i16 << 4u8 == 0i16, 1001);
    assert!(0i32 << 4u8 == 0i32, 1002);
    assert!(0i64 << 1u8 == 0i64, 1003);
    assert!(0i128 << 127u8 == 0i128, 1004);
    assert!(0i256 << 255u8 == 0i256, 1005);

    assert!(0i8 >> 4u8 == 0i8, 1100);
    assert!(0i16 >> 4u8 == 0i16, 1101);
    assert!(0i32 >> 4u8 == 0i32, 1102);
    assert!(0i64 >> 1u8 == 0i64, 1103);
    assert!(0i128 >> 127u8 == 0i128, 1104);
    assert!(0i256 >> 255u8 == 0i256, 1105);
}
}

// Shifting by 0 bits results in the same number.
//# run
module 12::m {
fun main() {
    assert!(100i8 << 0u8 == 100i8, 2000);
    assert!(43i16 << 0u8 == 43i16, 2001);
    assert!(10000i32 << 0u8 == 10000i32, 2002);
    assert!(43i64 << 0u8 == 43i64, 2003);

    assert!(100i8 >> 0u8 == 100i8, 2100);
    assert!(43i16 >> 0u8 == 43i16, 2101);
    assert!(10000i32 >> 0u8 == 10000i32, 2102);
    assert!(43i64 >> 0u8 == 43i64, 2103);

    // Negative values
    assert!(-100i8 << 0u8 == -100i8, 2200);
    assert!(-43i64 << 0u8 == -43i64, 2201);
    assert!(-100i8 >> 0u8 == -100i8, 2300);
    assert!(-43i64 >> 0u8 == -43i64, 2301);
}
}

// shl/shr by 1 equivalent to mul/div by 2.
//# run
module 13::m {
fun main() {
    assert!(1i8 << 1u8 == 2i8, 3000);
    assert!(7i16 << 1u8 == 14i16, 3001);
    assert!(7i32 << 1u8 == 14i32, 3002);
    assert!(1000i64 << 1u8 == 2000i64, 3003);
    assert!(1000i128 << 1u8 == 2000i128, 3004);
    assert!(1000i256 << 1u8 == 2000i256, 3005);

    assert!(1i8 >> 1u8 == 0i8, 3100);
    assert!(7i16 >> 1u8 == 3i16, 3101);
    assert!(7i32 >> 1u8 == 3i32, 3102);
    assert!(1000i64 >> 1u8 == 500i64, 3103);
    assert!(1000i128 >> 1u8 == 500i128, 3104);
    assert!(1000i256 >> 1u8 == 500i256, 3105);
}
}

// Right shift of negative numbers (arithmetic shift, sign-extending).
//# run
module 14::m {
fun main() {
    assert!(-1i8 >> 1u8 == -1i8, 4000);
    assert!(-2i8 >> 1u8 == -1i8, 4001);
    assert!(-4i8 >> 1u8 == -2i8, 4002);
    assert!(-128i8 >> 7u8 == -1i8, 4003);

    assert!(-1i16 >> 1u8 == -1i16, 4100);
    assert!(-2i16 >> 1u8 == -1i16, 4101);
    assert!(-32768i16 >> 15u8 == -1i16, 4102);

    assert!(-1i32 >> 1u8 == -1i32, 4200);
    assert!(-2i32 >> 1u8 == -1i32, 4201);

    assert!(-1i64 >> 1u8 == -1i64, 4300);
    assert!(-2i64 >> 1u8 == -1i64, 4301);

    assert!(-1i128 >> 1u8 == -1i128, 4400);
    assert!(-2i128 >> 1u8 == -1i128, 4401);

    assert!(-1i256 >> 1u8 == -1i256, 4500);
    assert!(-2i256 >> 1u8 == -1i256, 4501);
}
}

// Left shift: overflowing results are truncated.
//# run
module 15::m {
fun main() {
    assert!(7i8 << 5u8 == -32i8, 5000);
    assert!(1i8 << 7u8 == -128i8, 5001);
}
}

// Some random tests.
//# run
module 16::m {
fun main() {
    assert!(5i8 << 2u8 == 20i8, 6000);
    assert!(12i16 << 4u8 == 192i16, 6001);
    assert!(123i32 << 1u8 == 246i32, 6002);
    assert!(123i64 << 1u8 == 246i64, 6003);
}
}
