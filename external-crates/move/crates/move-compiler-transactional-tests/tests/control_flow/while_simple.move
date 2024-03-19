//# run
module 0x42::m {
fun main() {
    let x = 0;
    while (x < 5) x = x + 1;
    assert!(x == 5, 42);
}
}
