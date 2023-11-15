//# run
module 0x42::m {
fun main() {
    while (true) {
        while (true) return ();
        assert!(false, 42);
    };
    assert!(false, 42);
}
}
