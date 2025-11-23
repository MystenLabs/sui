//# run
module 0x42::m {
fun main() {
    let x: u64;
    if (true)
        x = 3
    else {
        x = 5
    };
    x;
}
}
