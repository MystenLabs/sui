script {
fun main(cond: bool) {
    let x;
    let y;
    if (cond) {
        x = 1;
        y = move x;
        y;
    } else {
        x = 0;
    };
    assert!(x == 5, 42);
}
}

// check: COPYLOC_UNAVAILABLE_ERROR
