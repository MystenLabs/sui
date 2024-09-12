module 0x42::m;

fun test(): u64 {
    if (true) {
        5
    } else {
        abort 0
    }
}
