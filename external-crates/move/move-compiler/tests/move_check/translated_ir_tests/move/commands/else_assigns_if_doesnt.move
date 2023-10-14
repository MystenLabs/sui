script {
fun main(cond: bool) {
    let x;
    let y;
    if (cond) {
        y = 0;
    } else {
        x = 42;
        x;
    };
    assert!(y == 0, 42);
}
}

// check: COPYLOC_UNAVAILABLE_ERROR
