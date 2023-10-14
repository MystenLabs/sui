script {
fun main(cond: bool) {
    let x = 0;
    let y = if (cond) 0 else move x; y;
    assert!(x == 0, 42);
}
}

// check: COPYLOC_UNAVAILABLE_ERROR
