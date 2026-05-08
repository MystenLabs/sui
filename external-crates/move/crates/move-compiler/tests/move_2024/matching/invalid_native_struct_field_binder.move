module 0x42::m {
    public native struct N;

    fun t(n: &N): u64 {
        match (n) {
            N { x } => x,
        }
    }
}
