//# run
script {
fun main() {
    assert!(0u32 + 0u32 == 0u32, 1000);
    assert!(0u32 + 1u32 == 1u32, 1001);
    assert!(1u32 + 1u32 == 2u32, 1002);

    assert!(13u32 + 67u32 == 80u32, 1100);
    assert!(100u32 + 10u32 == 110u32, 1101);

    assert!(0u32 + 4294967295u32 == 4294967295u32, 1200);
    assert!(1u32 + 4294967294u32 == 4294967295u32, 1201);
    assert!(5u32 + 4294967290u32 == 4294967295u32, 1202);
}
}

//# run
script {
fun main() {
    // should fail
    1u32 + 4294967295u32;
}
}

//# run
script {
fun main() {
    // should fail
    4294967295u32 + 4294967295u32;
}
}

//# run
script {
fun main() {
    assert!(0u32 - 0u32 == 0u32, 2000);
    assert!(1u32 - 0u32 == 1u32, 2001);
    assert!(1u32 - 1u32 == 0u32, 2002);

    assert!(52u32 - 13u32 == 39u32, 2100);
    assert!(100u32 - 10u32 == 90u32, 2101);

    assert!(4294967295u32 - 4294967295u32 == 0u32, 2200);
    assert!(5u32 - 1u32 - 4u32 == 0u32, 2201);
}
}

//# run
script {
fun main() {
    // should fail
    0u32 - 1u32;
}
}

//# run
script {
fun main() {
    // should fail
    54u32 - 100u32;
}
}


//# run
script {
fun main() {
    assert!(0u32 * 0u32 == 0u32, 3000);
    assert!(1u32 * 0u32 == 0u32, 3001);
    assert!(1u32 * 1u32 == 1u32, 3002);

    assert!(6u32 * 7u32 == 42u32, 3100);
    assert!(10u32 * 10u32 == 100u32, 3101);

    assert!(2147483647u32 * 2u32 == 4294967294u32, 3200);
}
}

//# run
script {
fun main() {
    // should fail
    1147483647u32 * 2147483647u32;
}
}

//# run
script {
fun main() {
    // should fail
    1147483647u32 * 2u32;
}
}



//# run
script {
fun main() {
    assert!(0u32 / 1u32 == 0u32, 4000);
    assert!(1u32 / 1u32 == 1u32, 4001);
    assert!(1u32 / 2u32 == 0u32, 4002);

    assert!(6u32 / 3u32 == 2u32, 4100);
    assert!(4294967294u32 / 13u32 == 330382099u32, 4101);

    assert!(4294967294u32 / 4294967295u32 == 0u32, 4200);
    assert!(4294967295u32 / 4294967294u32 == 1u32, 4201);
}
}

//# run
script {
fun main() {
    // should fail
    0u32 / 0u32;
}
}
// check: ARITHMETIC_ERROR

//# run
script {
fun main() {
    1u32 / 0u32;
}
}

//# run
script {
fun main() {
    // should fail
    4294967294u32 / 0u32;
}
}


//# run
script {
fun main() {
    assert!(0u32 % 1u32 == 0u32, 5000);
    assert!(1u32 % 1u32 == 0u32, 5001);
    assert!(1u32 % 2u32 == 1u32, 5002);

    assert!(8u32 % 3u32 == 2u32, 5100);
    assert!(4294967294u32 % 1234u32 == 678u32, 5101);

    assert!(4294967294u32 % 4294967295u32 == 4294967294u32, 5200);
    assert!(4294967294u32 % 4294967294u32 == 0u32, 5201);
}
}

//# run
script {
fun main() {
    // should fail
    0u32 % 0u32;
}
}

//# run
script {
fun main() {
    // should fail
    1u32 % 0u32;
}
}

//# run
script {
fun main() {
    // should fail
    4294967294u32 % 0u32;
}
}
