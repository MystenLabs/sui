// Test: Named blocks and labeled control flow
// EBNF: BlockLabel, BreakExp, ContinueExp, ReturnExp, ExpOrBlock
module 0x42::named_blocks;

fun labeled_block(): u64 {
    'outer: {
        let x = 10;
        'inner: {
            if (x > 5) {
                return 'outer x * 2
            };
            x + 1
        }
    }
}

fun labeled_loop(): u64 {
    let mut i = 0;
    let mut sum = 0;
    'outer: loop {
        i = i + 1;
        if (i > 10) break 'outer sum;
        'inner: loop {
            sum = sum + i;
            break 'inner
        }
    }
}

fun labeled_while(): u64 {
    let mut i = 0;
    'counting: while (i < 100) {
        i = i + 1;
        if (i == 50) break 'counting;
        if (i % 2 == 0) continue 'counting;
    };
    i
}

fun return_from_block(): u64 {
    'block: {
        if (true) return 'block 42;
        0
    }
}

fun nested_named(): u64 {
    'a: {
        'b: {
            'c: {
                return 'a 1
            }
        }
    }
}
