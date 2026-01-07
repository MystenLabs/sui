//# run
module 0x42::m {
fun main() {
    let x = 0u64;
    let z = 0u64;
    let y: u64;
    while (x < 3) {
        x = x + 1;
        y = 0;
        while (y < 7) {
            y = y + 1;
            z = z + 1;
        }
    };
    assert!(z == 21, 42)
}
}
