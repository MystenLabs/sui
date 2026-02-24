// Test: Various control flow expressions
// EBNF: ControlExp, IfExp, WhileExp, LoopExp, BreakExp, ContinueExp, ReturnExp, AbortExp
module 0x42::control_flow;

fun if_else(x: u64): u64 {
    if (x > 10) {
        x * 2
    } else if (x > 5) {
        x + 10
    } else {
        x
    }
}

fun if_without_else(x: u64): u64 {
    let mut result = x;
    if (x > 10) {
        result = result * 2;
    };
    result
}

fun while_loop(): u64 {
    let mut i = 0;
    let mut sum = 0;
    while (i < 10) {
        sum = sum + i;
        i = i + 1;
    };
    sum
}

fun loop_with_break(): u64 {
    let mut i = 0;
    loop {
        i = i + 1;
        if (i >= 10) break i;
    }
}

fun loop_with_continue(): u64 {
    let mut i = 0;
    let mut sum = 0;
    loop {
        i = i + 1;
        if (i > 10) break;
        if (i % 2 == 0) continue;
        sum = sum + i;
    };
    sum
}

fun early_return(x: u64): u64 {
    if (x == 0) return 0;
    if (x == 1) return 1;
    x * x
}

fun abort_example(x: u64): u64 {
    if (x == 0) abort 0;
    if (x > 100) abort 1;
    x
}

fun nested_loops(): u64 {
    let mut result = 0;
    let mut i = 0;
    while (i < 5) {
        let mut j = 0;
        while (j < 5) {
            result = result + i * j;
            j = j + 1;
        };
        i = i + 1;
    };
    result
}
