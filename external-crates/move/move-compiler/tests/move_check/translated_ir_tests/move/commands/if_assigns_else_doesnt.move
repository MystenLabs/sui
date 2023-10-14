script {
fun main(cond: bool) {
    let x;
    let y;
    if (cond) {
        x = 42;
    } else {
        y = 0;
        y;
    };
    assert!(x == 42, 42);
}
}

// check: COPYLOC_UNAVAILABLE_ERROR
