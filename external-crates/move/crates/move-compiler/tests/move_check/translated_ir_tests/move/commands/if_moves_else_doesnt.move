module 0x42::m {
fun main() {
    let x = 0;
    let y = if (true) move x else 0;
    y;
    assert!(x == 0u64, 42);
}
}

// check: COPYLOC_UNAVAILABLE_ERROR
