module 0x42::m;

fun test(cond: bool): u64 {
    'a: {
        loop {
            if (cond) { return 'a 5 }
        }
    }
}
