//# run
module 0x42::m {
fun main() {
    let x = 0u64;
    let y = 1u64;
    if (false) {
        y = move x;
        y;
        return ()
    };
    y;

    assert!(x == 0, 42);
}
}
