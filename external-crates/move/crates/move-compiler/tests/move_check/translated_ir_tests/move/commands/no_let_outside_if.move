module 0x42::m {
fun main() {
    if (true) {
        y = 5;
    } else {
        y = 0;
    };
    assert!(y == 5, 42);
}
}
