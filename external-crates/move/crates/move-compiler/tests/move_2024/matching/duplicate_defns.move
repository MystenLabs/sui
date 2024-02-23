module 0x42::a {
    public enum X {
        A { x: u64 },
        B { x: u64, y: u64 },
        C(u64, bool, bool),
        D,
    }

    public enum X {
        A(u64),
        B(u64)
    }

    public struct X(u64, u64, u64)
}
