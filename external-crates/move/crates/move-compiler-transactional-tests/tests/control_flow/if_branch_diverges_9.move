//# run
module 0x42::m {
fun main() {
    let b = false;
    loop {
        if (b) { if (b) continue }
        else break;
    };
}
}
