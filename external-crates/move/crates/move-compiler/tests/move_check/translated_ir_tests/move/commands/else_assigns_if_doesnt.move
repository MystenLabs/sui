module 0x42::m {
fun main() {
    let x;
    let y;
    if (true) {
        y = 0u64;
    } else {
        x = 42u64;
        x;
    };
    assert!(y == 0, 42);
}
}

// check: COPYLOC_UNAVAILABLE_ERROR
