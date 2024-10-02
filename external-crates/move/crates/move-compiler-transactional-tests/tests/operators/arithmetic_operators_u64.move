//# run
module 1::m {
fun main() {
    assert!(0u64 + 0u64 == 0u64, 1000);
    assert!(0u64 + 1u64 == 1u64, 1001);
    assert!(1u64 + 1u64 == 2u64, 1002);

    assert!(13u64 + 67u64 == 80u64, 1100);
    assert!(100u64 + 10u64 == 110u64, 1101);

    assert!(0u64 + 18446744073709551615u64 == 18446744073709551615u64, 1200);
    assert!(1u64 + 18446744073709551614u64 == 18446744073709551615u64, 1201);
    assert!(5u64 + 18446744073709551610u64 == 18446744073709551615u64, 1202);
}
}

//# run
module 2::m {
fun main() {
    // should fail
    1u64 + 18446744073709551615u64;
}
}

//# run
module 3::m {
fun main() {
    // should fail
    12000000000000000000u64 + 10000000000000000000u64;
}
}



//# run
module 4::m {
fun main() {
    assert!(0u64 - 0u64 == 0u64, 2000);
    assert!(1u64 - 0u64 == 1u64, 2001);
    assert!(1u64 - 1u64 == 0u64, 2002);

    assert!(52u64 - 13u64 == 39u64, 2100);
    assert!(100u64 - 10u64 == 90u64, 2101);

    assert!(18446744073709551615u64 - 18446744073709551615u64 == 0u64, 2200);
    assert!(5u64 - 1u64 - 4u64 == 0u64, 2201);
}
}

//# run
module 5::m {
fun main() {
    // should fail
    0u64 - 1u64;
}
}

//# run
module 6::m {
fun main() {
    // should fail
    54u64 - 100u64;
}
}


//# run
module 7::m {
fun main() {
    assert!(0u64 * 0u64 == 0u64, 3000);
    assert!(1u64 * 0u64 == 0u64, 3001);
    assert!(1u64 * 1u64 == 1u64, 3002);

    assert!(6u64 * 7u64 == 42u64, 3100);
    assert!(10u64 * 10u64 == 100u64, 3101);

    assert!(9223372036854775807u64 * 2u64 == 18446744073709551614u64, 3200);
}
}

//# run
module 8::m {
fun main() {
    // should fail
    4294967296u64 * 4294967296u64;
}
}

//# run
module 9::m {
fun main() {
    // should fail
    9223372036854775808 * 2u64;
}
}



//# run
module 10::m {
fun main() {
    assert!(0u64 / 1u64 == 0u64, 4000);
    assert!(1u64 / 1u64 == 1u64, 4001);
    assert!(1u64 / 2u64 == 0u64, 4002);

    assert!(6u64 / 3u64 == 2u64, 4100);
    assert!(18446744073709551615u64 / 13131u64 == 1404824009878116u64, 4101);

    assert!(18446744073709551614u64 / 18446744073709551615u64 == 0u64, 4200);
    assert!(18446744073709551615u64 / 18446744073709551615u64 == 1u64, 4201);
}
}

//# run
module 11::m {
fun main() {
    // should fail
    0u64 / 0u64;
}
}
// check: ARITHMETIC_ERROR

//# run
module 12::m {
fun main() {
    1u64 / 0u64;
}
}

//# run
module 13::m {
fun main() {
    // should fail
    18446744073709551615u64 / 0u64;
}
}


//# run
module 14::m {
fun main() {
    assert!(0u64 % 1u64 == 0u64, 5000);
    assert!(1u64 % 1u64 == 0u64, 5001);
    assert!(1u64 % 2u64 == 1u64, 5002);

    assert!(8u64 % 3u64 == 2u64, 5100);
    assert!(18446744073709551615u64 % 13131u64 == 10419u64, 5101);

    assert!(18446744073709551614u64 % 18446744073709551615u64 == 18446744073709551614u64, 5200);
    assert!(18446744073709551615u64 % 18446744073709551615u64 == 0u64, 5201);
}
}

//# run
module 15::m {
fun main() {
    // should fail
    0u64 % 0u64;
}
}

//# run
module 16::m {
fun main() {
    // should fail
    1u64 % 0u64;
}
}

//# run
module 17::m {
fun main() {
    // should fail
    18446744073709551615u64 % 0u64;
}
}
