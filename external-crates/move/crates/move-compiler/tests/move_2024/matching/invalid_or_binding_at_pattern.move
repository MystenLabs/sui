module 0x42::m {
    public enum E has drop { A(u64), B }

    fun t(e: E): u64 {
        match (e) {
            x @ (E::B(0) | E::A(_)) => match (x) {
                _ => 0,
            },
            _ => 1,
        }
    }
}
