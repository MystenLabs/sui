//# init --edition development

// Basic if/else on sign of value
//# run
module 1::m {
fun main() {
    let x: i32 = -5i32;
    let result: u64;
    if (x < 0i32) {
        result = 1;
    } else {
        result = 2;
    };
    assert!(result == 1, 1000);
}
}

// Positive value takes else branch
//# run
module 2::m {
fun main() {
    let x: i32 = 5i32;
    let result: u64;
    if (x < 0i32) {
        result = 1;
    } else {
        result = 2;
    };
    assert!(result == 2, 2000);
}
}

// Zero is not negative
//# run
module 3::m {
fun main() {
    let x: i32 = 0i32;
    let result: u64;
    if (x < 0i32) {
        result = 1;
    } else if (x == 0i32) {
        result = 0;
    } else {
        result = 2;
    };
    assert!(result == 0, 3000);
}
}

// Nested conditionals with signed comparisons
//# run
module 4::m {
fun main() {
    let x: i32 = -50i32;
    let result: u64;
    if (x < -100i32) {
        result = 1;
    } else if (x < 0i32) {
        result = 2;
    } else if (x < 100i32) {
        result = 3;
    } else {
        result = 4;
    };
    assert!(result == 2, 4000);
}
}

// Boundary test: i8 minimum
//# run
module 5::m {
fun main() {
    let x: i8 = -128i8;
    let result: u64;
    if (x == -128i8) {
        result = 1;
    } else {
        result = 0;
    };
    assert!(result == 1, 5000);
}
}

// Boundary test: i8 maximum
//# run
module 6::m {
fun main() {
    let x: i8 = 127i8;
    let result: u64;
    if (x == 127i8) {
        result = 1;
    } else {
        result = 0;
    };
    assert!(result == 1, 6000);
}
}

// Conditional with multiple signed types
//# run
module 7::m {
fun main() {
    let a: i8 = -1i8;
    let b: i16 = -1i16;
    let c: i32 = -1i32;
    let d: i64 = -1i64;
    let count: u64 = 0;
    if (a < 0i8) { count = count + 1; };
    if (b < 0i16) { count = count + 1; };
    if (c < 0i32) { count = count + 1; };
    if (d < 0i64) { count = count + 1; };
    assert!(count == 4, 7000);
}
}

// Conditional comparing negative to positive
//# run
module 8::m {
fun main() {
    let neg: i32 = -10i32;
    let pos: i32 = 10i32;
    assert!(neg < pos, 8000);
    assert!(!(neg > pos), 8001);
    assert!(!(neg == pos), 8002);
    assert!(neg != pos, 8003);
    assert!(neg <= pos, 8004);
    assert!(!(neg >= pos), 8005);
}
}

// If expression returning signed value
//# run
module 9::m {
fun main() {
    let x: i32 = -5i32;
    let abs_x: i32 = if (x < 0i32) { 0i32 - x } else { x };
    assert!(abs_x == 5i32, 9000);

    let y: i32 = 5i32;
    let abs_y: i32 = if (y < 0i32) { 0i32 - y } else { y };
    assert!(abs_y == 5i32, 9001);
}
}

// Boundary: i16 min/max in conditionals
//# run
module 10::m {
fun main() {
    let min: i16 = -32768i16;
    let max: i16 = 32767i16;
    assert!(min < max, 10000);
    assert!(min < 0i16, 10001);
    assert!(max > 0i16, 10002);
    assert!(min != max, 10003);
}
}
