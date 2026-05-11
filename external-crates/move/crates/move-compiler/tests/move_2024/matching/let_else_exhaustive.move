// A non-refutable pattern (single-variant enum) makes the `else` branch
// provably unreachable after lowering. Pins what the compiler reports for
// that case — either a clean accept or an unreachable-arm warning.
module 0x42::m {

    public enum One<T> has drop {
        A(T),
    }

    fun exhaustive(): u64 {
        let One::A(x) = One::A(42u64) else { abort 0 };
        x
    }

}
