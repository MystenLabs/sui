module 0x42::m {

fun main() {
    // Unary negation is not supported.
    assert!(((1 - -2) == 3u64) && (-(1 - 2) == 1), 100);
}
}
