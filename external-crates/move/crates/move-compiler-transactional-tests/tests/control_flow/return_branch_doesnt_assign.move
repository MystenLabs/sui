//# run
module 0x42::m {
fun main() {
    let x;
    if (true) {
        return ()
    } else {
        x = 0u64
    };
    assert!(x == 5, 42);
}
}
