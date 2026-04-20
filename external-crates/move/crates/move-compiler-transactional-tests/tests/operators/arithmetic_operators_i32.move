//# init --edition development

// Addition: happy path
//# run
module 1::m {
fun main() {
    assert!(0i32 + 0i32 == 0i32, 1000);
    assert!(0i32 + 1i32 == 1i32, 1001);
    assert!(1i32 + 1i32 == 2i32, 1002);

    assert!(130000i32 + 670000i32 == 800000i32, 1100);
    assert!(1000000i32 + 100000i32 == 1100000i32, 1101);

    assert!(0i32 + 2147483647i32 == 2147483647i32, 1200);
    assert!(1i32 + 2147483646i32 == 2147483647i32, 1201);

    // Negative operands
    assert!(-1i32 + -1i32 == -2i32, 1300);
    assert!(-500000i32 + -500000i32 == -1000000i32, 1301);
    assert!(0i32 + -2147483648i32 == -2147483648i32, 1302);

    // Mixed signs
    assert!(1i32 + -1i32 == 0i32, 1400);
    assert!(1000000i32 + -500000i32 == 500000i32, 1401);
    assert!(-1000000i32 + 500000i32 == -500000i32, 1402);
    assert!(2147483647i32 + -2147483647i32 == 0i32, 1403);
}
}

// Addition: overflow (positive)
//# run
module 2::m {
fun main() {
    // should fail
    2147483647i32 + 1i32;
}
}

// Addition: overflow (negative)
//# run
module 3::m {
fun main() {
    // should fail
    -2147483648i32 + -1i32;
}
}

// Subtraction: happy path
//# run
module 4::m {
fun main() {
    assert!(0i32 - 0i32 == 0i32, 2000);
    assert!(1i32 - 0i32 == 1i32, 2001);
    assert!(1i32 - 1i32 == 0i32, 2002);

    assert!(520000i32 - 130000i32 == 390000i32, 2100);
    assert!(1000000i32 - 100000i32 == 900000i32, 2101);

    assert!(2147483647i32 - 2147483647i32 == 0i32, 2200);

    // Negative operands
    assert!(-1i32 - -1i32 == 0i32, 2300);
    assert!(-500000i32 - -300000i32 == -200000i32, 2301);

    // Mixed signs
    assert!(0i32 - 1i32 == -1i32, 2400);
    assert!(-1i32 - 1i32 == -2i32, 2401);
    assert!(500000i32 - 1000000i32 == -500000i32, 2402);

    // Boundary
    assert!(-2147483648i32 - 0i32 == -2147483648i32, 2500);
    assert!(-1i32 - 2147483647i32 == -2147483648i32, 2501);
}
}

// Subtraction: overflow (positive)
//# run
module 5::m {
fun main() {
    // should fail
    2147483647i32 - -1i32;
}
}

// Subtraction: overflow (negative)
//# run
module 6::m {
fun main() {
    // should fail
    -2147483648i32 - 1i32;
}
}

// Multiplication: happy path
//# run
module 7::m {
fun main() {
    assert!(0i32 * 0i32 == 0i32, 3000);
    assert!(1i32 * 0i32 == 0i32, 3001);
    assert!(1i32 * 1i32 == 1i32, 3002);

    assert!(600i32 * 700i32 == 420000i32, 3100);
    assert!(1000i32 * 1000i32 == 1000000i32, 3101);

    // Negative operands
    assert!(-1i32 * 1i32 == -1i32, 3200);
    assert!(-1i32 * -1i32 == 1i32, 3201);
    assert!(-600i32 * 700i32 == -420000i32, 3202);
    assert!(600i32 * -700i32 == -420000i32, 3203);
    assert!(-600i32 * -700i32 == 420000i32, 3204);

    // Boundary
    assert!(1i32 * 2147483647i32 == 2147483647i32, 3300);
    assert!(-1i32 * 2147483647i32 == -2147483647i32, 3301);
    assert!(1i32 * -2147483648i32 == -2147483648i32, 3302);
}
}

// Multiplication: overflow
//# run
module 8::m {
fun main() {
    // should fail
    1073741824i32 * 2i32;
}
}

// Multiplication: overflow (negative * negative)
//# run
module 9::m {
fun main() {
    // should fail
    -2147483648i32 * -1i32;
}
}

// Division: happy path
//# run
module 10::m {
fun main() {
    assert!(0i32 / 1i32 == 0i32, 4000);
    assert!(1i32 / 1i32 == 1i32, 4001);
    assert!(1i32 / 2i32 == 0i32, 4002);

    assert!(60000i32 / 300i32 == 200i32, 4100);
    assert!(2147483647i32 / 7i32 == 306783378i32, 4101);

    // Negative operands
    assert!(-60000i32 / 300i32 == -200i32, 4200);
    assert!(60000i32 / -300i32 == -200i32, 4201);
    assert!(-60000i32 / -300i32 == 200i32, 4202);

    // Truncation toward zero
    assert!(7i32 / 2i32 == 3i32, 4300);
    assert!(-7i32 / 2i32 == -3i32, 4301);
    assert!(7i32 / -2i32 == -3i32, 4302);
    assert!(-7i32 / -2i32 == 3i32, 4303);

    // Boundary
    assert!(2147483647i32 / 2147483647i32 == 1i32, 4400);
    assert!(-2147483648i32 / 1i32 == -2147483648i32, 4401);
}
}

// Division by zero
//# run
module 11::m {
fun main() {
    // should fail
    0i32 / 0i32;
}
}

//# run
module 12::m {
fun main() {
    // should fail
    1i32 / 0i32;
}
}

//# run
module 13::m {
fun main() {
    // should fail
    -2147483648i32 / 0i32;
}
}

// Modulo: happy path
//# run
module 14::m {
fun main() {
    assert!(0i32 % 1i32 == 0i32, 5000);
    assert!(1i32 % 1i32 == 0i32, 5001);
    assert!(1i32 % 2i32 == 1i32, 5002);

    assert!(800i32 % 30i32 == 20i32, 5100);
    assert!(2147483647i32 % 7i32 == 1i32, 5101);

    // Negative operands
    assert!(-8i32 % 3i32 == -2i32, 5200);
    assert!(8i32 % -3i32 == 2i32, 5201);
    assert!(-8i32 % -3i32 == -2i32, 5202);

    // Boundary
    assert!(-2147483648i32 % 1i32 == 0i32, 5300);
    assert!(-2147483648i32 % 2147483647i32 == -1i32, 5301);
}
}

// Modulo by zero
//# run
module 15::m {
fun main() {
    // should fail
    0i32 % 0i32;
}
}

//# run
module 16::m {
fun main() {
    // should fail
    1i32 % 0i32;
}
}

//# run
module 17::m {
fun main() {
    // should fail
    -2147483648i32 % 0i32;
}
}
