module 0x42::m {
fun main() {
    let x = 5u64;
    let ref;
    if (true) {
        ref = &x;
    };
    assert!(*move ref == 5, 42);
}
}

// check: MOVELOC_UNAVAILABLE_ERROR
