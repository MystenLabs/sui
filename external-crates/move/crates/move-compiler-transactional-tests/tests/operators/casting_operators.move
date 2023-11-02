// Casting to u8.
//# run
script {
fun main() {
    // 0 remains unchanged.
    assert!((0u8 as u8) == 0u8, 1000);
    assert!((0u64 as u8) == 0u8, 1001);
    assert!((0u128 as u8) == 0u8, 1002);
    assert!((0u16 as u8) == 0u8, 1000);
    assert!((0u32 as u8) == 0u8, 1001);
    assert!((0u256 as u8) == 0u8, 1002);

    // Random small number unchanged.
    assert!((21u8 as u8) == 21u8, 1100);
    assert!((21u64 as u8) == 21u8, 1101);
    assert!((21u128 as u8) == 21u8, 1102);
    assert!((21u16 as u8) == 21u8, 1100);
    assert!((21u32 as u8) == 21u8, 1101);
    assert!((21u256 as u8) == 21u8, 1102);

    // Max representable values remain unchanged.
    assert!((255u8 as u8) == 255u8, 1200);
    assert!((255u64 as u8) == 255u8, 1201);
    assert!((255u128 as u8) == 255u8, 1202);
    assert!((255u16 as u8) == 255u8, 1200);
    assert!((255u32 as u8) == 255u8, 1201);
    assert!((255u256 as u8) == 255u8, 1202);
}
}

// Casting to u16.
//# run
script {
fun main() {
    // 0 remains unchanged.
    assert!((0u8 as u16) == 0u16, 1000);
    assert!((0u64 as u16) == 0u16, 1001);
    assert!((0u128 as u16) == 0u16, 1002);
    assert!((0u16 as u16) == 0u16, 1000);
    assert!((0u32 as u16) == 0u16, 1001);
    assert!((0u256 as u16) == 0u16, 1002);

    // Random small number unchanged.
    assert!((21u8 as u16) == 21u16, 1100);
    assert!((21u64 as u16) == 21u16, 1101);
    assert!((21u128 as u16) == 21u16, 1102);
    assert!((21u16 as u16) == 21u16, 1100);
    assert!((21u32 as u16) == 21u16, 1101);
    assert!((21u256 as u16) == 21u16, 1102);

    // Max representable values remain unchanged.
    assert!((255u8 as u16) == 255u16, 1200);
    assert!((65535u64 as u16) == 65535u16, 1201);
    assert!((65535u128 as u16) == 65535u16, 1202);
    assert!((65535u16 as u16) == 65535u16, 1200);
    assert!((65535u32 as u16) == 65535u16, 1201);
    assert!((65535u256 as u16) == 65535u16, 1202);
}
}

// Casting to u32.
//# run
script {
fun main() {
    // 0 remains unchanged.
    assert!((0u8 as u32) == 0u32, 1000);
    assert!((0u64 as u32) == 0u32, 1001);
    assert!((0u128 as u32) == 0u32, 1002);
    assert!((0u16 as u32) == 0u32, 1000);
    assert!((0u32 as u32) == 0u32, 1001);
    assert!((0u256 as u32) == 0u32, 1002);

    // Random small number unchanged.
    assert!((21u8 as u32) == 21u32, 1100);
    assert!((21u64 as u32) == 21u32, 1101);
    assert!((21u128 as u32) == 21u32, 1102);
    assert!((21u16 as u32) == 21u32, 1100);
    assert!((21u32 as u32) == 21u32, 1101);
    assert!((21u256 as u32) == 21u32, 1102);

    // Max representable values remain unchanged.
    assert!((255u8 as u32) == 255u32, 1200);
    assert!((4294967295u64 as u32) == 4294967295u32, 1201);
    assert!((4294967295u128 as u32) == 4294967295u32, 1202);
    assert!((65535u16 as u32) == 65535u32, 1200);
    assert!((4294967295u32 as u32) == 4294967295u32, 1201);
    assert!((4294967295u256 as u32) == 4294967295u32, 1202);
}
}

// Casting to u64.
//# run
script {
fun main() {
    // 0 remains unchanged.
    assert!((0u8 as u64) == 0u64, 2000);
    assert!((0u64 as u64) == 0u64, 2001);
    assert!((0u128 as u64) == 0u64, 2002);
    assert!((0u16 as u64) == 0u64, 2000);
    assert!((0u32 as u64) == 0u64, 2001);
    assert!((0u256 as u64) == 0u64, 2002);

    // Random small number unchanged.
    assert!((21u8 as u64) == 21u64, 2100);
    assert!((21u64 as u64) == 21u64, 2101);
    assert!((21u128 as u64) == 21u64, 2102);
    assert!((21u16 as u64) == 21u64, 2100);
    assert!((21u32 as u64) == 21u64, 2101);
    assert!((21u256 as u64) == 21u64, 2102);

    // Max representable values remain unchanged.
    assert!((255u8 as u64) == 255u64, 2200);
    assert!((18446744073709551615u64 as u64) == 18446744073709551615u64, 2201);
    assert!((18446744073709551615u128 as u64) == 18446744073709551615u64, 2202);
    assert!((65535u16 as u64) == 65535u64, 2200);
    assert!((4294967295u32 as u64) == 4294967295u64, 2201);
    assert!((18446744073709551615u256 as u64) == 18446744073709551615u64, 2202);
}
}

// Casting to u128.
//# run
script {
fun main() {
    // 0 remains unchanged.
    assert!((0u8 as u128) == 0u128, 3000);
    assert!((0u64 as u128) == 0u128, 3001);
    assert!((0u128 as u128) == 0u128, 3002);
    assert!((0u16 as u128) == 0u128, 3000);
    assert!((0u32 as u128) == 0u128, 3001);
    assert!((0u256 as u128) == 0u128, 3002);

    // Random small number unchanged.
    assert!((21u8 as u128) == 21u128, 3100);
    assert!((21u64 as u128) == 21u128, 3101);
    assert!((21u128 as u128) == 21u128, 3102);
    assert!((21u16 as u128) == 21u128, 3100);
    assert!((21u32 as u128) == 21u128, 3101);
    assert!((21u256 as u128) == 21u128, 3102);

    // Max representable values remain unchanged.
    assert!((255u8 as u128) == 255u128, 3200);
    assert!((18446744073709551615u64 as u128) == 18446744073709551615u128, 3201);
    assert!((340282366920938463463374607431768211455u128 as u128) == 340282366920938463463374607431768211455u128, 3202);
    assert!((65535u16 as u128) == 65535u128, 2200);
    assert!((4294967295u32 as u128) == 4294967295u128, 2201);
    assert!((340282366920938463463374607431768211455u256 as u128) == 340282366920938463463374607431768211455u128, 3202);
}
}

// Casting to u256.
//# run
script {
fun main() {
    // 0 remains unchanged.
    assert!((0u8 as u256) == 0u256, 3000);
    assert!((0u64 as u256) == 0u256, 3001);
    assert!((0u128 as u256) == 0u256, 3002);
    assert!((0u16 as u256) == 0u256, 3000);
    assert!((0u32 as u256) == 0u256, 3001);
    assert!((0u256 as u256) == 0u256, 3002);

    // Random small number unchanged.
    assert!((21u8 as u256) == 21u256, 3100);
    assert!((21u64 as u256) == 21u256, 3101);
    assert!((21u128 as u256) == 21u256, 3102);
    assert!((21u16 as u256) == 21u256, 3100);
    assert!((21u32 as u256) == 21u256, 3101);
    assert!((21u256 as u256) == 21u256, 3102);

    // Max representable values remain unchanged.
    assert!((255u8 as u256) == 255u256, 3200);
    assert!((18446744073709551615u64 as u256) == 18446744073709551615u256, 3201);
    assert!((340282366920938463463374607431768211455u128 as u256) == 340282366920938463463374607431768211455u256, 3202);
    assert!((65535u16 as u256) == 65535u256, 2200);
    assert!((4294967295u32 as u256) == 4294967295u256, 2201);
    assert!((115792089237316195423570985008687907853269984665640564039457584007913129639935u256 as u256) == 115792089237316195423570985008687907853269984665640564039457584007913129639935u256, 3202);
}
}

// Casting to u8, overflowing.
//# run
script {
fun main() {
    // should fail
    (256u64 as u8);
}
}

//# run
script {
fun main() {
    // should fail
    (303u64 as u8);
}
}

//# run
script {
fun main() {
    // should fail
    (256u128 as u8);
}
}

//# run
script {
fun main() {
    // should fail
    (56432u128 as u8);
}
}

//# run
script {
fun main() {
    // should fail
    (18446744073709551615u64 as u8);
}
}

//# run
script {
fun main() {
    // should fail
    (340282366920938463463374607431768211455u128 as u8);
}
}

//# run
script {
fun main() {
    // should fail
    (2561u16 as u8);
}
}

//# run
script {
fun main() {
    // should fail
    (65532u16 as u8);
}
}

//# run
script {
fun main() {
    // should fail
    (256123u32 as u8);
}
}

//# run
script {
fun main() {
    // should fail
    (11579208923731619542357098500868790785326998466564056403945758400791312963993u256 as u8);
}
}

// Casting to u16, overflowing.
//# run
script {
fun main() {
    // should fail
    (256343532u64 as u16);
}
}

//# run
script {
fun main() {
    // should fail
    (3564603u64 as u16);
}
}

//# run
script {
fun main() {
    // should fail
    (256666765790535666u128 as u16);
}
}

//# run
script {
fun main() {
    // should fail
    (256765735666u128 as u16);
}
}

//# run
script {
fun main() {
    // should fail
    (18446744073709551615u64 as u16);
}
}

//# run
script {
fun main() {
    // should fail
    (340282366920938463463374607431768211455u128 as u16);
}
}

//# run
script {
fun main() {
    // should fail
    (429496729u32 as u16);
}
}

//# run
script {
fun main() {
    // should fail
    (42949629u32 as u16);
}
}

//# run
script {
fun main() {
    // should fail
    (115792089237316195423570985008687907853269984665640564039457584007913129639u256 as u16);
}
}

// Casting to u32, overflowing.
//# run
script {
fun main() {
    // should fail
    (4294967295644u64 as u32);
}
}

//# run
script {
fun main() {
    // should fail
    (3564699003u64 as u32);
}
}

//# run
script {
fun main() {
    // should fail
    (256666765790535666u128 as u32);
}
}

//# run
script {
fun main() {
    // should fail
    (25676573566896u128 as u32);
}
}

//# run
script {
fun main() {
    // should fail
    (18446744073709551615u64 as u32);
}
}

//# run
script {
fun main() {
    // should fail
    (340282366920938463463374607431768211455u128 as u32);
}
}

//# run
script {
fun main() {
    // should fail
    (115792089237316195423570985008687907853269984665640564039457584007913129639u256 as u32);
}
}

// Casting to u64, overflowing.
//# run
script {
fun main() {
    // should fail
    (18446744073709551616u128 as u64);
}
}

//# run
script {
fun main() {
    // should fail
    (18446744073709551647u128 as u64);
}
}

//# run
script {
fun main() {
    // should fail
    (340282366920938463463374607431768211455u128 as u64);
}
}
