//# run
module 0x42::m {
fun main() {
    if (true) return ();
    assert!(false, 42);
}
}
