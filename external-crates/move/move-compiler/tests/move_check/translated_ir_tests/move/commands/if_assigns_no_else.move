script {
fun main(cond: bool) {
    let x;
    if (cond) x = 42;
    assert!(x == 42, 42);
}
}

// check: COPYLOC_UNAVAILABLE_ERROR
