//# init --edition development

// Addition: happy path
//# run
module 1::m {
fun main() {
    assert!(0i16 + 0i16 == 0i16, 1000);
    assert!(0i16 + 1i16 == 1i16, 1001);
    assert!(1i16 + 1i16 == 2i16, 1002);

    assert!(1300i16 + 6700i16 == 8000i16, 1100);
    assert!(10000i16 + 1000i16 == 11000i16, 1101);

    assert!(0i16 + 32767i16 == 32767i16, 1200);
    assert!(1i16 + 32766i16 == 32767i16, 1201);

    // Negative operands
    assert!(-1i16 + -1i16 == -2i16, 1300);
    assert!(-5000i16 + -5000i16 == -10000i16, 1301);
    assert!(0i16 + -32768i16 == -32768i16, 1302);

    // Mixed signs
    assert!(1i16 + -1i16 == 0i16, 1400);
    assert!(10000i16 + -5000i16 == 5000i16, 1401);
    assert!(-10000i16 + 5000i16 == -5000i16, 1402);
    assert!(32767i16 + -32767i16 == 0i16, 1403);
}
}

// Addition: overflow (positive)
//# run
module 2::m {
fun main() {
    // should fail
    32767i16 + 1i16;
}
}

// Addition: overflow (negative)
//# run
module 3::m {
fun main() {
    // should fail
    -32768i16 + -1i16;
}
}

// Subtraction: happy path
//# run
module 4::m {
fun main() {
    assert!(0i16 - 0i16 == 0i16, 2000);
    assert!(1i16 - 0i16 == 1i16, 2001);
    assert!(1i16 - 1i16 == 0i16, 2002);

    assert!(5200i16 - 1300i16 == 3900i16, 2100);
    assert!(10000i16 - 1000i16 == 9000i16, 2101);

    assert!(32767i16 - 32767i16 == 0i16, 2200);

    // Negative operands
    assert!(-1i16 - -1i16 == 0i16, 2300);
    assert!(-5000i16 - -3000i16 == -2000i16, 2301);

    // Mixed signs
    assert!(0i16 - 1i16 == -1i16, 2400);
    assert!(-1i16 - 1i16 == -2i16, 2401);
    assert!(5000i16 - 10000i16 == -5000i16, 2402);

    // Boundary
    assert!(-32768i16 - 0i16 == -32768i16, 2500);
    assert!(-1i16 - 32767i16 == -32768i16, 2501);
}
}

// Subtraction: overflow (positive)
//# run
module 5::m {
fun main() {
    // should fail
    32767i16 - -1i16;
}
}

// Subtraction: overflow (negative)
//# run
module 6::m {
fun main() {
    // should fail
    -32768i16 - 1i16;
}
}

// Multiplication: happy path
//# run
module 7::m {
fun main() {
    assert!(0i16 * 0i16 == 0i16, 3000);
    assert!(1i16 * 0i16 == 0i16, 3001);
    assert!(1i16 * 1i16 == 1i16, 3002);

    assert!(60i16 * 70i16 == 4200i16, 3100);
    assert!(100i16 * 100i16 == 10000i16, 3101);

    // Negative operands
    assert!(-1i16 * 1i16 == -1i16, 3200);
    assert!(-1i16 * -1i16 == 1i16, 3201);
    assert!(-60i16 * 70i16 == -4200i16, 3202);
    assert!(60i16 * -70i16 == -4200i16, 3203);
    assert!(-60i16 * -70i16 == 4200i16, 3204);

    // Boundary
    assert!(1i16 * 32767i16 == 32767i16, 3300);
    assert!(-1i16 * 32767i16 == -32767i16, 3301);
    assert!(1i16 * -32768i16 == -32768i16, 3302);
}
}

// Multiplication: overflow
//# run
module 8::m {
fun main() {
    // should fail
    16384i16 * 2i16;
}
}

// Multiplication: overflow (negative * negative)
//# run
module 9::m {
fun main() {
    // should fail
    -32768i16 * -1i16;
}
}

// Division: happy path
//# run
module 10::m {
fun main() {
    assert!(0i16 / 1i16 == 0i16, 4000);
    assert!(1i16 / 1i16 == 1i16, 4001);
    assert!(1i16 / 2i16 == 0i16, 4002);

    assert!(600i16 / 3i16 == 200i16, 4100);
    assert!(32767i16 / 7i16 == 4681i16, 4101);

    // Negative operands
    assert!(-600i16 / 3i16 == -200i16, 4200);
    assert!(600i16 / -3i16 == -200i16, 4201);
    assert!(-600i16 / -3i16 == 200i16, 4202);

    // Truncation toward zero
    assert!(7i16 / 2i16 == 3i16, 4300);
    assert!(-7i16 / 2i16 == -3i16, 4301);
    assert!(7i16 / -2i16 == -3i16, 4302);
    assert!(-7i16 / -2i16 == 3i16, 4303);

    // Boundary
    assert!(32767i16 / 32767i16 == 1i16, 4400);
    assert!(-32768i16 / 1i16 == -32768i16, 4401);
}
}

// Division by zero
//# run
module 11::m {
fun main() {
    // should fail
    0i16 / 0i16;
}
}

//# run
module 12::m {
fun main() {
    // should fail
    1i16 / 0i16;
}
}

//# run
module 13::m {
fun main() {
    // should fail
    -32768i16 / 0i16;
}
}

// Modulo: happy path
//# run
module 14::m {
fun main() {
    assert!(0i16 % 1i16 == 0i16, 5000);
    assert!(1i16 % 1i16 == 0i16, 5001);
    assert!(1i16 % 2i16 == 1i16, 5002);

    assert!(800i16 % 30i16 == 20i16, 5100);
    assert!(32767i16 % 7i16 == 4i16, 5101);

    // Negative operands
    assert!(-8i16 % 3i16 == -2i16, 5200);
    assert!(8i16 % -3i16 == 2i16, 5201);
    assert!(-8i16 % -3i16 == -2i16, 5202);

    // Boundary
    assert!(-32768i16 % 1i16 == 0i16, 5300);
    assert!(-32768i16 % 32767i16 == -1i16, 5301);
}
}

// Modulo by zero
//# run
module 15::m {
fun main() {
    // should fail
    0i16 % 0i16;
}
}

//# run
module 16::m {
fun main() {
    // should fail
    1i16 % 0i16;
}
}

//# run
module 17::m {
fun main() {
    // should fail
    -32768i16 % 0i16;
}
}
