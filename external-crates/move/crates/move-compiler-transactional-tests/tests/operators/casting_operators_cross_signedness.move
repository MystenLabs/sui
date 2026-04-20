//# init --edition development

// Signed to unsigned: happy path
//# run
module 1::m {
fun main() {
    // Zero
    assert!((0i8 as u8) == 0u8, 1000);
    assert!((0i16 as u16) == 0u16, 1001);
    assert!((0i32 as u32) == 0u32, 1002);
    assert!((0i64 as u64) == 0u64, 1003);
    assert!((0i128 as u128) == 0u128, 1004);
    assert!((0i256 as u256) == 0u256, 1005);

    // Positive values
    assert!((42i8 as u8) == 42u8, 1100);
    assert!((42i16 as u16) == 42u16, 1101);
    assert!((42i32 as u32) == 42u32, 1102);
    assert!((42i64 as u64) == 42u64, 1103);
    assert!((42i128 as u128) == 42u128, 1104);
    assert!((42i256 as u256) == 42u256, 1105);

    // Max signed values
    assert!((127i8 as u8) == 127u8, 1200);
    assert!((32767i16 as u16) == 32767u16, 1201);
    assert!((2147483647i32 as u32) == 2147483647u32, 1202);
    assert!((9223372036854775807i64 as u64) == 9223372036854775807u64, 1203);

    // Signed to wider unsigned
    assert!((127i8 as u16) == 127u16, 1300);
    assert!((127i8 as u32) == 127u32, 1301);
    assert!((127i8 as u64) == 127u64, 1302);
    assert!((32767i16 as u32) == 32767u32, 1303);
    assert!((32767i16 as u64) == 32767u64, 1304);
}
}

// Signed to unsigned: negative value fails (same width)
//# run
module 2::m {
fun main() {
    // should fail
    (-1i8 as u8);
}
}

//# run
module 3::m {
fun main() {
    // should fail
    (-1i16 as u16);
}
}

//# run
module 4::m {
fun main() {
    // should fail
    (-1i32 as u32);
}
}

//# run
module 5::m {
fun main() {
    // should fail
    (-1i64 as u64);
}
}

//# run
module 6::m {
fun main() {
    // should fail
    (-1i128 as u128);
}
}

//# run
module 7::m {
fun main() {
    // should fail
    (-1i256 as u256);
}
}

// Signed to unsigned: MIN value fails
//# run
module 8::m {
fun main() {
    // should fail
    (-128i8 as u8);
}
}

//# run
module 9::m {
fun main() {
    // should fail
    (-128i8 as u64);
}
}

// Unsigned to signed: happy path
//# run
module 10::m {
fun main() {
    // Zero
    assert!((0u8 as i8) == 0i8, 2000);
    assert!((0u16 as i16) == 0i16, 2001);
    assert!((0u32 as i32) == 0i32, 2002);
    assert!((0u64 as i64) == 0i64, 2003);
    assert!((0u128 as i128) == 0i128, 2004);
    assert!((0u256 as i256) == 0i256, 2005);

    // Positive values within signed range
    assert!((42u8 as i8) == 42i8, 2100);
    assert!((42u16 as i16) == 42i16, 2101);
    assert!((42u32 as i32) == 42i32, 2102);
    assert!((42u64 as i64) == 42i64, 2103);
    assert!((42u128 as i128) == 42i128, 2104);
    assert!((42u256 as i256) == 42i256, 2105);

    // Max signed value from unsigned
    assert!((127u8 as i8) == 127i8, 2200);
    assert!((32767u16 as i16) == 32767i16, 2201);
    assert!((2147483647u32 as i32) == 2147483647i32, 2202);
    assert!((9223372036854775807u64 as i64) == 9223372036854775807i64, 2203);

    // Unsigned to wider signed
    assert!((255u8 as i16) == 255i16, 2300);
    assert!((255u8 as i32) == 255i32, 2301);
    assert!((255u8 as i64) == 255i64, 2302);
    assert!((65535u16 as i32) == 65535i32, 2303);
    assert!((65535u16 as i64) == 65535i64, 2304);
    assert!((4294967295u32 as i64) == 4294967295i64, 2305);
}
}

// Unsigned to signed: overflow (same width, value > MAX signed)
//# run
module 11::m {
fun main() {
    // should fail: 128 > 127 (i8 max)
    (128u8 as i8);
}
}

//# run
module 12::m {
fun main() {
    // should fail
    (255u8 as i8);
}
}

//# run
module 13::m {
fun main() {
    // should fail: 32768 > 32767 (i16 max)
    (32768u16 as i16);
}
}

//# run
module 14::m {
fun main() {
    // should fail
    (65535u16 as i16);
}
}

//# run
module 15::m {
fun main() {
    // should fail: 2147483648 > 2147483647 (i32 max)
    (2147483648u32 as i32);
}
}

//# run
module 16::m {
fun main() {
    // should fail
    (9223372036854775808u64 as i64);
}
}

//# run
module 17::m {
fun main() {
    // should fail
    (18446744073709551615u64 as i64);
}
}

//# run
module 18::m {
fun main() {
    // should fail
    (170141183460469231731687303715884105728u128 as i128);
}
}
