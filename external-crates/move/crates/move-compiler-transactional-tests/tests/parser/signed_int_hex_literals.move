//# init --edition development

// Hex literals for i8
//# run
module 1::m {
fun main() {
    assert!(0x0i8 == 0i8, 100);
    assert!(0x1i8 == 1i8, 101);
    assert!(0x0Fi8 == 15i8, 102);
    assert!(0x7Fi8 == 127i8, 103);
    assert!(0x2Ai8 == 42i8, 104);
    assert!(0x00i8 == 0i8, 105);
    assert!(0x007Fi8 == 127i8, 106);
}
}

// Hex literals for i16
//# run
module 2::m {
fun main() {
    assert!(0x0i16 == 0i16, 200);
    assert!(0x1i16 == 1i16, 201);
    assert!(0xFFi16 == 255i16, 202);
    assert!(0xFFFi16 == 4095i16, 203);
    assert!(0x7FFFi16 == 32767i16, 204);
    assert!(0x00FFi16 == 255i16, 205);
    assert!(0x007FFFi16 == 32767i16, 206);
}
}

// Hex literals for i32
//# run
module 3::m {
fun main() {
    assert!(0x0i32 == 0i32, 300);
    assert!(0x1i32 == 1i32, 301);
    assert!(0xFFFFi32 == 65535i32, 302);
    assert!(0x7FFFFFFFi32 == 2147483647i32, 303);
    assert!(0x0000FFFFi32 == 65535i32, 304);
    assert!(0x007FFFFFFFi32 == 2147483647i32, 305);
}
}

// Hex literals for i64
//# run
module 4::m {
fun main() {
    assert!(0x0i64 == 0i64, 400);
    assert!(0x1i64 == 1i64, 401);
    assert!(0xFFFFFFFFi64 == 4294967295i64, 402);
    assert!(0x7FFFFFFFFFFFFFFFi64 == 9223372036854775807i64, 403);
    assert!(0x00000000FFFFFFFFi64 == 4294967295i64, 404);
}
}

// Hex literals for i128
//# run
module 5::m {
fun main() {
    assert!(0x0i128 == 0i128, 500);
    assert!(0x1i128 == 1i128, 501);
    assert!(0xFFFFFFFFFFFFFFFFi128 == 18446744073709551615i128, 502);
    assert!(
        0x7FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFi128
            == 170141183460469231731687303715884105727i128,
        503,
    );
}
}

// Hex literals for i256
//# run
module 6::m {
fun main() {
    assert!(0x0i256 == 0i256, 600);
    assert!(0x1i256 == 1i256, 601);
    assert!(
        0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFi256
            == 340282366920938463463374607431768211455i256,
        602,
    );
    assert!(
        0x7FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFi256
            == 57896044618658097711785492504343953926634992332820282019728792003956564819967i256,
        603,
    );
}
}

// Hex with underscore separators
//# run
module 7::m {
fun main() {
    assert!(0x7_Fi8 == 127i8, 700);
    assert!(0x7F_FFi16 == 32767i16, 701);
    assert!(0x7FFF_FFFFi32 == 2147483647i32, 702);
    assert!(0x7FFF_FFFF_FFFF_FFFFi64 == 9223372036854775807i64, 703);
    assert!(0x0_0i8 == 0i8, 704);
    assert!(0x0__7Fi8 == 127i8, 705);
}
}

// Hex with negative values
//# run
module 8::m {
fun main() {
    assert!(-0x1i8 == -1i8, 800);
    assert!(-0x7Fi8 == -127i8, 801);
    assert!(-0x1i16 == -1i16, 802);
    assert!(-0x7FFFi16 == -32767i16, 803);
    assert!(-0x1i32 == -1i32, 804);
    assert!(-0x7FFFFFFFi32 == -2147483647i32, 805);
    assert!(-0x1i64 == -1i64, 806);
    assert!(-0x7FFFFFFFFFFFFFFFi64 == -9223372036854775807i64, 807);
    assert!(-0x1i128 == -1i128, 808);
    assert!(-0x1i256 == -1i256, 809);
}
}
