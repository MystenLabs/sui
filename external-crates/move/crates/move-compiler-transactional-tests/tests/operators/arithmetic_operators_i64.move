//# init --edition development

// Addition: happy path
//# run
module 1::m {
fun main() {
    assert!(0i64 + 0i64 == 0i64, 1000);
    assert!(0i64 + 1i64 == 1i64, 1001);
    assert!(1i64 + 1i64 == 2i64, 1002);

    assert!(1300000000i64 + 6700000000i64 == 8000000000i64, 1100);

    assert!(0i64 + 9223372036854775807i64 == 9223372036854775807i64, 1200);
    assert!(1i64 + 9223372036854775806i64 == 9223372036854775807i64, 1201);

    // Negative operands
    assert!(-1i64 + -1i64 == -2i64, 1300);
    assert!(-5000000000i64 + -5000000000i64 == -10000000000i64, 1301);
    assert!(0i64 + -9223372036854775808i64 == -9223372036854775808i64, 1302);

    // Mixed signs
    assert!(1i64 + -1i64 == 0i64, 1400);
    assert!(9223372036854775807i64 + -9223372036854775807i64 == 0i64, 1401);
}
}

// Addition: overflow (positive)
//# run
module 2::m {
fun main() {
    // should fail
    9223372036854775807i64 + 1i64;
}
}

// Addition: overflow (negative)
//# run
module 3::m {
fun main() {
    // should fail
    -9223372036854775808i64 + -1i64;
}
}

// Subtraction: happy path
//# run
module 4::m {
fun main() {
    assert!(0i64 - 0i64 == 0i64, 2000);
    assert!(1i64 - 0i64 == 1i64, 2001);
    assert!(1i64 - 1i64 == 0i64, 2002);

    assert!(9223372036854775807i64 - 9223372036854775807i64 == 0i64, 2200);

    // Negative operands
    assert!(-1i64 - -1i64 == 0i64, 2300);

    // Mixed signs
    assert!(0i64 - 1i64 == -1i64, 2400);
    assert!(-1i64 - 1i64 == -2i64, 2401);

    // Boundary
    assert!(-9223372036854775808i64 - 0i64 == -9223372036854775808i64, 2500);
    assert!(-1i64 - 9223372036854775807i64 == -9223372036854775808i64, 2501);
}
}

// Subtraction: overflow (positive)
//# run
module 5::m {
fun main() {
    // should fail
    9223372036854775807i64 - -1i64;
}
}

// Subtraction: overflow (negative)
//# run
module 6::m {
fun main() {
    // should fail
    -9223372036854775808i64 - 1i64;
}
}

// Multiplication: happy path
//# run
module 7::m {
fun main() {
    assert!(0i64 * 0i64 == 0i64, 3000);
    assert!(1i64 * 0i64 == 0i64, 3001);
    assert!(1i64 * 1i64 == 1i64, 3002);

    assert!(600000i64 * 700000i64 == 420000000000i64, 3100);

    // Negative operands
    assert!(-1i64 * 1i64 == -1i64, 3200);
    assert!(-1i64 * -1i64 == 1i64, 3201);
    assert!(-600000i64 * 700000i64 == -420000000000i64, 3202);

    // Boundary
    assert!(1i64 * 9223372036854775807i64 == 9223372036854775807i64, 3300);
    assert!(-1i64 * 9223372036854775807i64 == -9223372036854775807i64, 3301);
    assert!(1i64 * -9223372036854775808i64 == -9223372036854775808i64, 3302);
}
}

// Multiplication: overflow
//# run
module 8::m {
fun main() {
    // should fail
    4611686018427387904i64 * 2i64;
}
}

// Multiplication: overflow (negative * negative)
//# run
module 9::m {
fun main() {
    // should fail
    -9223372036854775808i64 * -1i64;
}
}

// Division: happy path
//# run
module 10::m {
fun main() {
    assert!(0i64 / 1i64 == 0i64, 4000);
    assert!(1i64 / 1i64 == 1i64, 4001);
    assert!(1i64 / 2i64 == 0i64, 4002);

    assert!(60000i64 / 300i64 == 200i64, 4100);

    // Negative operands
    assert!(-60000i64 / 300i64 == -200i64, 4200);
    assert!(60000i64 / -300i64 == -200i64, 4201);
    assert!(-60000i64 / -300i64 == 200i64, 4202);

    // Truncation toward zero
    assert!(7i64 / 2i64 == 3i64, 4300);
    assert!(-7i64 / 2i64 == -3i64, 4301);
    assert!(7i64 / -2i64 == -3i64, 4302);
    assert!(-7i64 / -2i64 == 3i64, 4303);

    // Boundary
    assert!(9223372036854775807i64 / 9223372036854775807i64 == 1i64, 4400);
    assert!(-9223372036854775808i64 / 1i64 == -9223372036854775808i64, 4401);
}
}

// Division by zero
//# run
module 11::m {
fun main() {
    // should fail
    0i64 / 0i64;
}
}

//# run
module 12::m {
fun main() {
    // should fail
    1i64 / 0i64;
}
}

//# run
module 13::m {
fun main() {
    // should fail
    -9223372036854775808i64 / 0i64;
}
}

// Modulo: happy path
//# run
module 14::m {
fun main() {
    assert!(0i64 % 1i64 == 0i64, 5000);
    assert!(1i64 % 1i64 == 0i64, 5001);
    assert!(1i64 % 2i64 == 1i64, 5002);

    assert!(800i64 % 30i64 == 20i64, 5100);

    // Negative operands
    assert!(-8i64 % 3i64 == -2i64, 5200);
    assert!(8i64 % -3i64 == 2i64, 5201);
    assert!(-8i64 % -3i64 == -2i64, 5202);

    // Boundary
    assert!(-9223372036854775808i64 % 1i64 == 0i64, 5300);
}
}

// Modulo by zero
//# run
module 15::m {
fun main() {
    // should fail
    0i64 % 0i64;
}
}

//# run
module 16::m {
fun main() {
    // should fail
    1i64 % 0i64;
}
}

//# run
module 17::m {
fun main() {
    // should fail
    -9223372036854775808i64 % 0i64;
}
}
