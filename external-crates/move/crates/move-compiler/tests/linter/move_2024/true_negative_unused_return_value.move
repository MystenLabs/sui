module 0x42::m;

fun pure(x: u64): u64 { x + 1 }

fun imm(_: &u64) {}

fun mutating(_: &mut u64): u64 { 0 }

// explicit `let _` discard is fine
fun t(b: bool): u64 {
    let _ = pure(1);
    let _x = pure(1);

    // used in return
    if (b) return pure(1);

    // has mut input
    mutating(&mut 0);

    // Used in expr
    let _ = pure(pure(1));
    imm(&pure(1));
    let _ = mutating(&mut pure(1));
    let _ = pure(if (b) pure(0) else pure(1));
    0
}

#[allow(unused_variable)]
fun t_unused_variable() {
    // Triggers unused variable, not the lint
    let x = pure(1);
}

#[allow(unused_assignment)]
fun t_unused_assignment() {
    // Triggers unused assignment, not the lint
    let mut x = pure(1);
    x = pure(2);
    let _ = x;
}
