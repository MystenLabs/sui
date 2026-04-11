//# init --edition development

// Addition: happy path
//# run
module 1::m {
fun main() {
    assert!(0i8 + 0i8 == 0i8, 1000);
    assert!(0i8 + 1i8 == 1i8, 1001);
    assert!(1i8 + 1i8 == 2i8, 1002);

    assert!(13i8 + 67i8 == 80i8, 1100);
    assert!(50i8 + 10i8 == 60i8, 1101);

    assert!(0i8 + 127i8 == 127i8, 1200);
    assert!(1i8 + 126i8 == 127i8, 1201);

    // Negative operands
    assert!(-1i8 + -1i8 == -2i8, 1300);
    assert!(-50i8 + -50i8 == -100i8, 1301);
    assert!(0i8 + -128i8 == -128i8, 1302);

    // Mixed signs
    assert!(1i8 + -1i8 == 0i8, 1400);
    assert!(100i8 + -50i8 == 50i8, 1401);
    assert!(-100i8 + 50i8 == -50i8, 1402);
    assert!(127i8 + -127i8 == 0i8, 1403);
}
}

// Addition: overflow (positive)
//# run
module 2::m {
fun main() {
    // should fail
    127i8 + 1i8;
}
}

// Addition: overflow (negative)
//# run
module 3::m {
fun main() {
    // should fail
    -128i8 + -1i8;
}
}

// Subtraction: happy path
//# run
module 4::m {
fun main() {
    assert!(0i8 - 0i8 == 0i8, 2000);
    assert!(1i8 - 0i8 == 1i8, 2001);
    assert!(1i8 - 1i8 == 0i8, 2002);

    assert!(52i8 - 13i8 == 39i8, 2100);
    assert!(100i8 - 10i8 == 90i8, 2101);

    assert!(127i8 - 127i8 == 0i8, 2200);
    assert!(5i8 - 1i8 - 4i8 == 0i8, 2201);

    // Negative operands
    assert!(-1i8 - -1i8 == 0i8, 2300);
    assert!(-50i8 - -30i8 == -20i8, 2301);

    // Mixed signs
    assert!(0i8 - 1i8 == -1i8, 2400);
    assert!(-1i8 - 1i8 == -2i8, 2401);
    assert!(50i8 - 100i8 == -50i8, 2402);

    // Boundary
    assert!(-128i8 - 0i8 == -128i8, 2500);
    assert!(-1i8 - 127i8 == -128i8, 2501);
}
}

// Subtraction: overflow (positive)
//# run
module 5::m {
fun main() {
    // should fail
    127i8 - -1i8;
}
}

// Subtraction: overflow (negative)
//# run
module 6::m {
fun main() {
    // should fail
    -128i8 - 1i8;
}
}

// Multiplication: happy path
//# run
module 7::m {
fun main() {
    assert!(0i8 * 0i8 == 0i8, 3000);
    assert!(1i8 * 0i8 == 0i8, 3001);
    assert!(1i8 * 1i8 == 1i8, 3002);

    assert!(6i8 * 7i8 == 42i8, 3100);
    assert!(10i8 * 10i8 == 100i8, 3101);

    // Negative operands
    assert!(-1i8 * 1i8 == -1i8, 3200);
    assert!(-1i8 * -1i8 == 1i8, 3201);
    assert!(-6i8 * 7i8 == -42i8, 3202);
    assert!(6i8 * -7i8 == -42i8, 3203);
    assert!(-6i8 * -7i8 == 42i8, 3204);

    // Boundary
    assert!(1i8 * 127i8 == 127i8, 3300);
    assert!(-1i8 * 127i8 == -127i8, 3301);
    assert!(1i8 * -128i8 == -128i8, 3302);
}
}

// Multiplication: overflow (positive)
//# run
module 8::m {
fun main() {
    // should fail
    64i8 * 2i8;
}
}

// Multiplication: overflow (negative * negative)
//# run
module 9::m {
fun main() {
    // should fail
    -128i8 * -1i8;
}
}

// Division: happy path
//# run
module 10::m {
fun main() {
    assert!(0i8 / 1i8 == 0i8, 4000);
    assert!(1i8 / 1i8 == 1i8, 4001);
    assert!(1i8 / 2i8 == 0i8, 4002);

    assert!(6i8 / 3i8 == 2i8, 4100);
    assert!(127i8 / 7i8 == 18i8, 4101);

    // Negative operands
    assert!(-6i8 / 3i8 == -2i8, 4200);
    assert!(6i8 / -3i8 == -2i8, 4201);
    assert!(-6i8 / -3i8 == 2i8, 4202);

    // Truncation toward zero
    assert!(7i8 / 2i8 == 3i8, 4300);
    assert!(-7i8 / 2i8 == -3i8, 4301);
    assert!(7i8 / -2i8 == -3i8, 4302);
    assert!(-7i8 / -2i8 == 3i8, 4303);

    // Boundary
    assert!(127i8 / 127i8 == 1i8, 4400);
    assert!(-128i8 / 1i8 == -128i8, 4401);
}
}

// Division: MIN / -1 overflow
//# run
module 18::m {
fun main() {
    // should fail: -128 / -1 = 128 which overflows i8
    -128i8 / -1i8;
}
}

// Division by zero
//# run
module 11::m {
fun main() {
    // should fail
    0i8 / 0i8;
}
}

//# run
module 12::m {
fun main() {
    // should fail
    1i8 / 0i8;
}
}

//# run
module 13::m {
fun main() {
    // should fail
    -128i8 / 0i8;
}
}

// Modulo: happy path
//# run
module 14::m {
fun main() {
    assert!(0i8 % 1i8 == 0i8, 5000);
    assert!(1i8 % 1i8 == 0i8, 5001);
    assert!(1i8 % 2i8 == 1i8, 5002);

    assert!(8i8 % 3i8 == 2i8, 5100);
    assert!(127i8 % 7i8 == 1i8, 5101);

    // Negative operands — sign of result matches sign of dividend
    assert!(-8i8 % 3i8 == -2i8, 5200);
    assert!(8i8 % -3i8 == 2i8, 5201);
    assert!(-8i8 % -3i8 == -2i8, 5202);

    // Boundary
    assert!(-128i8 % 1i8 == 0i8, 5300);
    assert!(-128i8 % 127i8 == -1i8, 5301);
}
}

// Modulo by zero
//# run
module 15::m {
fun main() {
    // should fail
    0i8 % 0i8;
}
}

//# run
module 16::m {
fun main() {
    // should fail
    1i8 % 0i8;
}
}

//# run
module 17::m {
fun main() {
    // should fail
    -128i8 % 0i8;
}
}
