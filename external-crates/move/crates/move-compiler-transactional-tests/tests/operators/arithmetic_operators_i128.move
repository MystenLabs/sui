//# init --edition development

// Addition: happy path
//# run
module 1::m {
fun main() {
    assert!(0i128 + 0i128 == 0i128, 1000);
    assert!(0i128 + 1i128 == 1i128, 1001);
    assert!(1i128 + 1i128 == 2i128, 1002);

    assert!(0i128 + 170141183460469231731687303715884105727i128 == 170141183460469231731687303715884105727i128, 1200);

    // Negative operands
    assert!(-1i128 + -1i128 == -2i128, 1300);
    assert!(0i128 + -170141183460469231731687303715884105728i128 == -170141183460469231731687303715884105728i128, 1302);

    // Mixed signs
    assert!(1i128 + -1i128 == 0i128, 1400);
    assert!(170141183460469231731687303715884105727i128 + -170141183460469231731687303715884105727i128 == 0i128, 1401);
}
}

// Addition: overflow (positive)
//# run
module 2::m {
fun main() {
    // should fail
    170141183460469231731687303715884105727i128 + 1i128;
}
}

// Addition: overflow (negative)
//# run
module 3::m {
fun main() {
    // should fail
    -170141183460469231731687303715884105728i128 + -1i128;
}
}

// Subtraction: happy path
//# run
module 4::m {
fun main() {
    assert!(0i128 - 0i128 == 0i128, 2000);
    assert!(1i128 - 0i128 == 1i128, 2001);
    assert!(1i128 - 1i128 == 0i128, 2002);

    assert!(170141183460469231731687303715884105727i128 - 170141183460469231731687303715884105727i128 == 0i128, 2200);

    // Negative operands
    assert!(-1i128 - -1i128 == 0i128, 2300);

    // Mixed signs
    assert!(0i128 - 1i128 == -1i128, 2400);

    // Boundary
    assert!(-170141183460469231731687303715884105728i128 - 0i128 == -170141183460469231731687303715884105728i128, 2500);
    assert!(-1i128 - 170141183460469231731687303715884105727i128 == -170141183460469231731687303715884105728i128, 2501);
}
}

// Subtraction: overflow (positive)
//# run
module 5::m {
fun main() {
    // should fail
    170141183460469231731687303715884105727i128 - -1i128;
}
}

// Subtraction: overflow (negative)
//# run
module 6::m {
fun main() {
    // should fail
    -170141183460469231731687303715884105728i128 - 1i128;
}
}

// Multiplication: happy path
//# run
module 7::m {
fun main() {
    assert!(0i128 * 0i128 == 0i128, 3000);
    assert!(1i128 * 0i128 == 0i128, 3001);
    assert!(1i128 * 1i128 == 1i128, 3002);

    assert!(600i128 * 700i128 == 420000i128, 3100);

    // Negative operands
    assert!(-1i128 * 1i128 == -1i128, 3200);
    assert!(-1i128 * -1i128 == 1i128, 3201);
    assert!(-600i128 * 700i128 == -420000i128, 3202);

    // Boundary
    assert!(1i128 * 170141183460469231731687303715884105727i128 == 170141183460469231731687303715884105727i128, 3300);
    assert!(-1i128 * 170141183460469231731687303715884105727i128 == -170141183460469231731687303715884105727i128, 3301);
    assert!(1i128 * -170141183460469231731687303715884105728i128 == -170141183460469231731687303715884105728i128, 3302);
}
}

// Multiplication: overflow
//# run
module 8::m {
fun main() {
    // should fail
    85070591730234615865843651857942052864i128 * 2i128;
}
}

// Multiplication: overflow (negative * negative)
//# run
module 9::m {
fun main() {
    // should fail
    -170141183460469231731687303715884105728i128 * -1i128;
}
}

// Division: happy path
//# run
module 10::m {
fun main() {
    assert!(0i128 / 1i128 == 0i128, 4000);
    assert!(1i128 / 1i128 == 1i128, 4001);
    assert!(1i128 / 2i128 == 0i128, 4002);

    assert!(60000i128 / 300i128 == 200i128, 4100);

    // Negative operands
    assert!(-60000i128 / 300i128 == -200i128, 4200);
    assert!(60000i128 / -300i128 == -200i128, 4201);
    assert!(-60000i128 / -300i128 == 200i128, 4202);

    // Truncation toward zero
    assert!(7i128 / 2i128 == 3i128, 4300);
    assert!(-7i128 / 2i128 == -3i128, 4301);
    assert!(7i128 / -2i128 == -3i128, 4302);
    assert!(-7i128 / -2i128 == 3i128, 4303);

    // Boundary
    assert!(170141183460469231731687303715884105727i128 / 170141183460469231731687303715884105727i128 == 1i128, 4400);
    assert!(-170141183460469231731687303715884105728i128 / 1i128 == -170141183460469231731687303715884105728i128, 4401);
}
}

// Division by zero
//# run
module 11::m {
fun main() {
    // should fail
    0i128 / 0i128;
}
}

//# run
module 12::m {
fun main() {
    // should fail
    1i128 / 0i128;
}
}

//# run
module 13::m {
fun main() {
    // should fail
    -170141183460469231731687303715884105728i128 / 0i128;
}
}

// Modulo: happy path
//# run
module 14::m {
fun main() {
    assert!(0i128 % 1i128 == 0i128, 5000);
    assert!(1i128 % 1i128 == 0i128, 5001);
    assert!(1i128 % 2i128 == 1i128, 5002);

    assert!(800i128 % 30i128 == 20i128, 5100);

    // Negative operands
    assert!(-8i128 % 3i128 == -2i128, 5200);
    assert!(8i128 % -3i128 == 2i128, 5201);
    assert!(-8i128 % -3i128 == -2i128, 5202);

    // Boundary
    assert!(-170141183460469231731687303715884105728i128 % 1i128 == 0i128, 5300);
}
}

// Modulo by zero
//# run
module 15::m {
fun main() {
    // should fail
    0i128 % 0i128;
}
}

//# run
module 16::m {
fun main() {
    // should fail
    1i128 % 0i128;
}
}

//# run
module 17::m {
fun main() {
    // should fail
    -170141183460469231731687303715884105728i128 % 0i128;
}
}
