// Runtime check: single-variant enum makes the let-else pattern provably
// non-refutable, so the else branch is unreachable. Pins that the success arm
// runs cleanly (i.e., no spurious abort or divergence is emitted).
//# init --edition 2024.beta

//# publish
module 0x42::m {

    public enum One<T> has drop {
        A(T),
    }

    public fun one_a(x: u64): One<u64> { One::A(x) }

    public fun exhaustive_inline(): u64 {
        let One::A(x) = One::A(42u64) else { abort 0 };
        x
    }

    public fun exhaustive_passthrough(o: One<u64>): u64 {
        let One::A(x) = o else { abort 0 };
        x
    }

}

//# run
module 0x43::main {

    fun main() {
        use 0x42::m::{one_a, exhaustive_inline, exhaustive_passthrough};

        assert!(exhaustive_inline() == 42, 1);
        assert!(exhaustive_passthrough(one_a(7)) == 7, 2);
    }
}
