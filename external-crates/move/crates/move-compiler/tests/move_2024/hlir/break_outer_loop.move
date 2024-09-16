module 0x42::m;

fun test0(): u64 {
    'a: loop {
        loop {
            break 'a 5
        }
    }
}

fun test1(): u64 {
    'a: loop {
        let x = loop {
            break 'a 5;
            break 10;
        };
        let _x = x;
    }
}
