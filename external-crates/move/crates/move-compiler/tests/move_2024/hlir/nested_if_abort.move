module 0x42::m;

fun test() {
    if (true) {
        if (true) abort 0 else abort 0
    } else {
        if (true) abort 0 else abort 0
    }
}
