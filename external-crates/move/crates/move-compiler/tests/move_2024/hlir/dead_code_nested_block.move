module 0x42::m;

fun test0(cond: bool): u64 {
    'a: {
        return 'a 5;
        0
    }
}

fun test1(): u64 {
    'a: {
        loop {
            return 'a 5
        };
        0
    }
}
