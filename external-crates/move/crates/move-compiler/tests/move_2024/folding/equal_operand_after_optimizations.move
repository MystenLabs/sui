// emit a warning during code generation for equal operands in binary operations that result
// in a constant value
// tests after several optimizations
module a::m;

fun equal_operands_inline(): bool {
    let x;
    loop {
        x = 1;
        break
    };
    let y = if (true) 1 else 1;
    x == y
}

fun equal_operands_beofre_and_after_inline() {
    let mut x;
    let mut y;
    loop {
        x = 1;
        y = 0;
        x == x;
        y == 0;
    }
}
