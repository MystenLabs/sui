//# run
module 0x42::m {
fun main() {
    if (true) {
        loop { break }
    } else {
        assert!(false, 42);
        return ()
    }
}
}
