module 0x42::m {
fun main() {
    let x;
    let y;
    if (true) x = 5u64 else ();
    if (true) y = 5;
    x == y;
}
}

// check: COPYLOC_UNAVAILABLE_ERROR
