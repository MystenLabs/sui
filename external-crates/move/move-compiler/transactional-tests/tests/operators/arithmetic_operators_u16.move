//# run
script {
fun main() {
    assert!(0u16 + 0u16 == 0u16, 1000);
    assert!(0u16 + 1u16 == 1u16, 1001);
    assert!(1u16 + 1u16 == 2u16, 1002);

    assert!(13u16 + 67u16 == 80u16, 1100);
    assert!(100u16 + 10u16 == 110u16, 1101);

    assert!(0u16 + 65535u16 == 65535u16, 1200);
    assert!(1u16 + 65534u16 == 65535u16, 1201);
    assert!(5u16 + 65530u16 == 65535u16, 1202);
}
}

//# run
script {
fun main() {
    // should fail
    1u16 + 65535u16;
}
}

//# run
script {
fun main() {
    // should fail
    65135u16 + 6555u16;
}
}

//# run
script {
fun main() {
    assert!(0u16 - 0u16 == 0u16, 2000);
    assert!(1u16 - 0u16 == 1u16, 2001);
    assert!(1u16 - 1u16 == 0u16, 2002);

    assert!(52u16 - 13u16 == 39u16, 2100);
    assert!(100u16 - 10u16 == 90u16, 2101);

    assert!(65535u16 - 65535u16 == 0u16, 2200);
    assert!(5u16 - 1u16 - 4u16 == 0u16, 2201);
}
}

//# run
script {
fun main() {
    // should fail
    0u16 - 1u16;
}
}

//# run
script {
fun main() {
    // should fail
    54u16 - 100u16;
}
}


//# run
script {
fun main() {
    assert!(0u16 * 0u16 == 0u16, 3000);
    assert!(1u16 * 0u16 == 0u16, 3001);
    assert!(1u16 * 1u16 == 1u16, 3002);

    assert!(6u16 * 7u16 == 42u16, 3100);
    assert!(10u16 * 10u16 == 100u16, 3101);

    assert!(32767u16 * 2u16 == 65534u16, 3200);
}
}

//# run
script {
fun main() {
    // should fail
    32767u16 * 32767u16;
}
}

//# run
script {
fun main() {
    // should fail
    33767u16 * 2u16;
}
}



//# run
script {
fun main() {
    assert!(0u16 / 1u16 == 0u16, 4000);
    assert!(1u16 / 1u16 == 1u16, 4001);
    assert!(1u16 / 2u16 == 0u16, 4002);

    assert!(6u16 / 3u16 == 2u16, 4100);
    assert!(65535u16 / 13u16 == 5041u16, 4101);

    assert!(65534u16 / 65535u16 == 0u16, 4200);
    assert!(65535u16 / 65534u16 == 1u16, 4201);
}
}

//# run
script {
fun main() {
    // should fail
    0u16 / 0u16;
}
}
// check: ARITHMETIC_ERROR

//# run
script {
fun main() {
    1u16 / 0u16;
}
}

//# run
script {
fun main() {
    // should fail
    65535u16 / 0u16;
}
}


//# run
script {
fun main() {
    assert!(0u16 % 1u16 == 0u16, 5000);
    assert!(1u16 % 1u16 == 0u16, 5001);
    assert!(1u16 % 2u16 == 1u16, 5002);

    assert!(8u16 % 3u16 == 2u16, 5100);
    assert!(65535u16 % 134u16 == 9u16, 5101);

    assert!(65534u16 % 65535u16 == 65534u16, 5200);
    assert!(65535u16 % 65535u16 == 0u16, 5201);
}
}

//# run
script {
fun main() {
    // should fail
    0u16 % 0u16;
}
}

//# run
script {
fun main() {
    // should fail
    1u16 % 0u16;
}
}

//# run
script {
fun main() {
    // should fail
    65535u16 % 0u16;
}
}
