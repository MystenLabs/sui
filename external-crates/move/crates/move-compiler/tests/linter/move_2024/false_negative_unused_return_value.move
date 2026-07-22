// Cases where the lint *could* warn but does not, by design of the analysis.
module 0x42::m;

fun pure(x: u64): u64 { x + 1 }

fun t() {
    pure(1) + 1; // no warning, even though the value is not "used"
}
