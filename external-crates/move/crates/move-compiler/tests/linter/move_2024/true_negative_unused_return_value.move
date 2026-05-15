module 0x42::m;

fun pure(x: u64): u64 { x + 1 }
fun mutating(x: &mut u64): u64 { *x = *x + 1; *x }

// explicit `let _` discard is fine
fun explicit_ignore() {
    let _ = pure(1);
}

// underscore-prefixed binding is conventionally unused; no warn
fun underscore_var() {
    let _x = pure(1);
}

// result used as the function's return value
fun returned(): u64 {
    pure(1)
}

// result used by another call (Move consumes it)
fun used_by_call(): u64 {
    let x = pure(1);
    pure(x)
}

// call has a `&mut` arg: the call may have side effects, do not warn
fun mut_arg_no_warn() {
    let mut y = 0;
    mutating(&mut y);
}

// used on every return path: no warn
fun used_on_every_path(b: bool): u64 {
    let x = pure(1);
    if (b) {
        return pure(x)
    };
    pure(x)
}

// used in some path and unused in another: do not warn (MaybeUnavailable at join)
fun maybe_used(b: bool) {
    let x = pure(1);
    if (b) {
        let _ = pure(x);
    };
}

// shadowing: each x has its own scope; the inner x is used
fun shadow(): u64 {
    let x = pure(1);
    let _ = x;
    {
        let x = pure(2);
        x
    }
}

// reference returns are tracked but consuming them by another call counts as use
fun ref_return(): u64 {
    let r = &10u64;
    *r
}
