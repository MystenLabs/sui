script {
fun main(cond: bool) {
    let x = 0;
    let y = if (cond) move x else 0;
    y;
    assert!(x == 0, 42);
}
}

// check: COPYLOC_UNAVAILABLE_ERROR
