module 0x42::m;

fun test0(cond: bool): u64 {
    'a: {
        if (cond) { return 'a 5 };
        0
    }
}

fun test1(cond: bool): u64 {
    'a: {
        loop {
            if (cond) { return 'a 5 };
        };
        0
    }
}

fun test2(cond: bool): u64 {
    'a: loop {
        loop {
            if (cond) { break 'a 5 };
        }
    }
}
