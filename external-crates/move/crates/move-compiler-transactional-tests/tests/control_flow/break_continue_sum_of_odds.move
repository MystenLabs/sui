//# run
module 0x42::m {
fun main() {
    let x = 0u64;
    let y = 0u64;
    loop {
        if (x < 10) {
            x = x + 1;
            if (x % 2 == 0) continue;
            y = y + x
        } else {
            break
        }
    };
    assert!(y == 25, 42);
}
}
