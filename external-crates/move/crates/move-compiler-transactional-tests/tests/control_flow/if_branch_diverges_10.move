//# run
module 0x42::m {
fun main() {
    if (true) {
        loop return ()
    } else {
        assert!(false, 42);
        return ()
    }
}
}
