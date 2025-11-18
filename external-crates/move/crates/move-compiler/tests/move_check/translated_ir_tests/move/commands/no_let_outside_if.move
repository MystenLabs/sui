module 0x42::m {
fun main() {
    if (true) {
        y = 5u64;
    } else {
        y = 0u64;
    };
    assert!(y == 5, 42);
}
}
