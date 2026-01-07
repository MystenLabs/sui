//# run
module 0x42::m {
fun main() {
    let x = 0u64;
    while (true) {
        x = x + 1;
        break
    };
    assert!(x == 1, 42);
}
}
