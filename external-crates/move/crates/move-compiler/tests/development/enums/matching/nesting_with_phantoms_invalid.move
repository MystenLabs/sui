module 0x42::a {
    public enum X {
        A { x: u64 },
        B { x: u64, y: u64 },
        C(u64, bool, bool),
        D,
    }

    public enum Y {
        A(X),
        B(S)
    }

    public struct S { }

    public struct T { x: X, y: Y }

    public struct Mixed { s: S, t: T, x: X , y: Y }

    public struct PS<phantom T> { s: S , x: X, t: T}

    public enum PE<phantom T> {
        One { s: S, x: X },
        Two(S, X, T),
        Three,
    }

    public struct NPS<phantom T> { s: PS<T>, e: PE<T>, x: X }

    public enum NPE<phantom T> {
        One { s: PS<T>, q: PE<T>, x: X},
        Two(PS<T>, PE<T>, X),
        Three,
    }
}
