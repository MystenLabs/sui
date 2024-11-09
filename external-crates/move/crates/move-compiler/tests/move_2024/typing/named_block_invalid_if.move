module 0x42::m;

fun test(cond: bool): u64 {
    'a: {
        if (cond) { return 'a 5 }
    }
}
