module 0x42::m {
fun main() {
    let x;
    let y;
    if (true) {
        x = 1u64;
        y = move x;
        y;
    } else {
        x = 0;
    };
    assert!(x == 5, 42);
}
}

// check: COPYLOC_UNAVAILABLE_ERROR
