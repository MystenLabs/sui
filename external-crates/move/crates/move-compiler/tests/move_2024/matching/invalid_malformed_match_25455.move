module 0x0::M {
    public enum C {
        R,
    }

    public enum N {
        A(C),
        B,
    }

    fun f(n: N): u64 {
        match (n) {
            N::A'x) => 1,
            N::A(_) => 2,
            N::B => 0,
        }
    }
}
