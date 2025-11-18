module 0x42::m {
fun main() {
    let x;
    if (true) x = 42u64;
    assert!(x == 42, 42);
}
}

// check: COPYLOC_UNAVAILABLE_ERROR
