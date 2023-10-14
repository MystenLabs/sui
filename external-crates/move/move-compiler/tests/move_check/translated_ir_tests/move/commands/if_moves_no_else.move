script {
fun main(cond: bool) {
    let x = 0;
    if (cond) {
        let y = move x;
        y;
    };
    assert!(x == 0, 42);
}
}

// check: COPYLOC_UNAVAILABLE_ERROR
