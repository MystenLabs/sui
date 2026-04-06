//# init --edition development

// Negation: happy path
//# run
module 1::m {
fun main() {
    // Negate zero
    assert!(-0i8 == 0i8, 1000);
    assert!(-0i16 == 0i16, 1001);
    assert!(-0i32 == 0i32, 1002);
    assert!(-0i64 == 0i64, 1003);
    assert!(-0i128 == 0i128, 1004);
    assert!(-0i256 == 0i256, 1005);

    // Negate positive
    assert!(-1i8 == -1i8, 1100);
    assert!(-1i16 == -1i16, 1101);
    assert!(-1i32 == -1i32, 1102);
    assert!(-1i64 == -1i64, 1103);
    assert!(-1i128 == -1i128, 1104);
    assert!(-1i256 == -1i256, 1105);

    // Negate MAX
    assert!(-127i8 == -127i8, 1200);
    assert!(-32767i16 == -32767i16, 1201);
    assert!(-2147483647i32 == -2147483647i32, 1202);
    assert!(-9223372036854775807i64 == -9223372036854775807i64, 1203);

    // Double negation
    let x: i8 = 42i8;
    let y: i8 = -x;
    assert!(y == -42i8, 1300);
    let z: i8 = -y;
    assert!(z == 42i8, 1301);

    let a: i64 = 123456789i64;
    let b: i64 = -a;
    assert!(b == -123456789i64, 1302);
    let c: i64 = -b;
    assert!(c == 123456789i64, 1303);
}
}

// Negate MIN overflows: i8
//# run
module 2::m {
fun main() {
    // should fail: -(-128) overflows i8
    let x: i8 = -128i8;
    -x;
}
}

// Negate MIN overflows: i16
//# run
module 3::m {
fun main() {
    // should fail
    let x: i16 = -32768i16;
    -x;
}
}

// Negate MIN overflows: i32
//# run
module 4::m {
fun main() {
    // should fail
    let x: i32 = -2147483648i32;
    -x;
}
}

// Negate MIN overflows: i64
//# run
module 5::m {
fun main() {
    // should fail
    let x: i64 = -9223372036854775808i64;
    -x;
}
}

// Negate MIN overflows: i128
//# run
module 6::m {
fun main() {
    // should fail
    let x: i128 = -170141183460469231731687303715884105728i128;
    -x;
}
}

// Negate MIN overflows: i256
//# run
module 7::m {
fun main() {
    // should fail
    let x: i256 = -57896044618658097711785492504343953926634992332820282019728792003956564819968i256;
    -x;
}
}
