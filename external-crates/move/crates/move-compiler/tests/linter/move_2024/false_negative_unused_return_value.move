// Cases where the lint *should* warn but does not, by design of the analysis.
module 0x42::m;

fun pure(x: u64): u64 { x + 1 }

// Operators discard their operands' tracking. The arithmetic itself has no effect, but the
// statement-discard is not flagged because the framework's default `BinopExp` evaluation
// returns a plain default value rather than propagating `Fresh`.
fun binop_discard() {
    (pure(1) + pure(2)); // no warn, but both calls are effectively unused
}

// Reassignment to the same local overwrites the prior `Bound` value; the previous call's
// tracking is lost, so its discard is silently dropped.
fun overwrite() {
    let mut x = pure(1); // no warn: x is later overwritten before being used
    x = pure(2);
    let _ = x;
}
