script {
fun main(cond: bool) {
    let x: u64;
    if (cond) () else x = 100;
    assert!(x == 100, 42);
}
}

// check: COPYLOC_UNAVAILABLE_ERROR
