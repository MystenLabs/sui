//# init --edition development

// While loop counting from negative to positive
//# run
module 1::m {
fun main() {
    let i: i32 = -5i32;
    while (i < 5i32) {
        i = i + 1i32;
    };
    assert!(i == 5i32, 1);
}
}

// While loop counting down from positive to negative
//# run
module 2::m {
fun main() {
    let i: i32 = 5i32;
    while (i > -5i32) {
        i = i - 1i32;
    };
    assert!(i == -5i32, 2);
}
}

// Loop with negative step (decrement by 2)
//# run
module 3::m {
fun main() {
    let i: i64 = 10i64;
    while (i > -10i64) {
        i = i - 2i64;
    };
    assert!(i == -10i64, 3);
}
}

// Accumulator with negative values
//# run
module 4::m {
fun main() {
    let sum: i32 = 0i32;
    let i: i32 = -5i32;
    while (i <= 5i32) {
        sum = sum + i;
        i = i + 1i32;
    };
    // Sum of -5..=5 is 0
    assert!(sum == 0i32, 4);
}
}

// Accumulator summing only negative values in a range
//# run
module 5::m {
fun main() {
    let sum: i32 = 0i32;
    let i: i32 = -5i32;
    while (i < 5i32) {
        if (i < 0i32) {
            sum = sum + i;
        };
        i = i + 1i32;
    };
    // Sum of -5 + -4 + -3 + -2 + -1 = -15
    assert!(sum == -15i32, 5);
}
}

// Loop with i8, testing near boundary
//# run
module 6::m {
fun main() {
    let i: i8 = -10i8;
    let count: i8 = 0i8;
    while (i < 10i8) {
        count = count + 1i8;
        i = i + 1i8;
    };
    assert!(count == 20i8, 6);
}
}

// Nested loop with signed integers
//# run
module 7::m {
fun main() {
    let sum: i32 = 0i32;
    let i: i32 = -2i32;
    while (i <= 2i32) {
        let j: i32 = -2i32;
        while (j <= 2i32) {
            sum = sum + i * j;
            j = j + 1i32;
        };
        i = i + 1i32;
    };
    // By symmetry, sum of i*j for i,j in -2..=2 is 0
    assert!(sum == 0i32, 7);
}
}

// Break out of loop on negative condition
//# run
module 8::m {
fun main() {
    let i: i32 = 0i32;
    while (true) {
        i = i - 1i32;
        if (i <= -10i32) break;
    };
    assert!(i == -10i32, 8);
}
}
