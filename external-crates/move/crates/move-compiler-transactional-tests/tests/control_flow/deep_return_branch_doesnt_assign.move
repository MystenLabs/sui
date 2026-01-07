//# run
module 0x42::m {
fun main() {
    let x: u64;
    if (true) {
        if (false) return () else return ()
    } else {
        x = 0;
    };
    assert!(x == 5, 42);
}
}
