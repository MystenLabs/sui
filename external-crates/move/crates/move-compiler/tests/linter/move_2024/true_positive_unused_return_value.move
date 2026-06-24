module 0x42::m;

public enum E has drop {
    A,
    B,
}

fun pure(x: u64): u64 { x + 1 }

fun pure2(x: &u64): (u64, &u64) { (*x + 2, x) }

public macro fun apply<$T>($body: || -> $T): $T { $body() }

// direct statement-discard
#[allow(dead_code)]
fun direct(b: bool, e: &E) {
    pure(1); // warn
    pure2(&0); // warn once, even though two values are ignored
    if (b) pure(1) else pure(2); // warn x2
    match (e) {
        E::A => pure(1), // warn
        E::B => pure(2), // warn
    };
    loop {
        if (b) break pure(1); // warn
    };
    'l: {
        if (b) {
            return 'l pure(1) // warn
        };
        pure(2) // warn
    };
    apply!(|| pure(1)); // warn
    'l: {
        apply!(|| return 'l pure(1)); // warn
        0 // unreachable
    };
    pure2(&pure(0)); // warn for the outer pure2
}

#[allow(dead_code)]
fun after_branch(cond: bool) {
    if (cond) return;
    pure(1); // warn
    apply!(|| pure(2)); // warn
    'l: {
        apply!(|| return 'l pure(1)); // warn
        0 // unreachable
    };
    if (cond) { if (cond) pure(1) else pure(2) } else { 0 }; // warn
}
