//# run
module 0x42::m {
fun main() {
    let x = 0u64;
    if (true) x = 3;
    x;
}
}
