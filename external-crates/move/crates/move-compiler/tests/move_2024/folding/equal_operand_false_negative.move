// emit a warning during code generation for equal operands in binary operations that result
// in a constant value
// these could produce warnings if the compiler handled these cases a bit better, in particular
// future optimizations could help.
// NOTE: Please move these out of this test once they start producing warnings

module a::m;

fun simple(x: u64, b: bool) {
    &1 == 1;
    (*&1) == 1;
    *&x == copy x;
    b && b;
    b || b;
}

fun optimizations() {
    let x;
    loop {
        x = 1;
        break
    };
    let y = if (true) 1 else 1;
    x == y;
    x == 1; // using x a second time produces a warning
}
