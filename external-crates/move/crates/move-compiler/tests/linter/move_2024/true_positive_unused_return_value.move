module 0x42::m;

public enum E has drop { A, B }

fun pure(x: u64): u64 { x + 1 }
fun pure2(x: &u64): (u64, &u64) { (*x + 2, x) }

public macro fun apply<$T>($body: || -> $T): $T { $body() }

// direct statement-discard
fun direct() {
    pure(1); // warn
}

// if/else as a discarded statement -- both arms are pure calls
fun if_else_statement(b: bool) {
    if (b) pure(1) else pure(2); // warn x2
}

// match as a discarded statement
fun match_statement(e: E) {
    match (e) {
        E::A => pure(1), // warn
        E::B => pure(2), // warn
    };
}

// loop with break that yields a pure call's value, statement-discarded
fun loop_break_statement(b: bool) {
    loop {
        if (b) break pure(1); // warn
    };
}

// named block whose value comes from a labeled break, statement-discarded
fun named_block_statement(b: bool) {
    'l: {
        if (b) {
            return 'l pure(1) // warn
        };
        pure(2) // warn
    };
}

// macro call statement-discarded; inlines to a discarded `pure(1)`
fun macro_direct() {
    apply!(|| pure(1)); // warn
}

// named labeled break from inside a macro's lambda; the enclosing 'l block is discarded
#[allow(dead_code)]
fun macro_named_break() {
    'l: {
        apply!(|| return 'l pure(1)); // warn
        0 // unreachable
    };
}
