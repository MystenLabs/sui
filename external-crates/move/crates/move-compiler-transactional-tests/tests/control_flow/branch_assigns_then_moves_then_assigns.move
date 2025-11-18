//# run
module 0x42::m {
fun main() {
    let x: u64;
    let y: u64;
    if (true) {
        x = 1;
        y = move x;
        x = 5;
        y;
    } else {
        x = 0;
    };
    assert!(copy x == 5, 42);
}
}
