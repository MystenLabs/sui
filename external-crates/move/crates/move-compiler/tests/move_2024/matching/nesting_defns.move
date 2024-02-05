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

    public struct PS<T> { s: S, t: T , x: X }

    public enum PE<T> {
        One { s: S, t: T, x: X },
        Two(S, T, X),
        Three,
    }

    public struct NPS<T> { s: PS<T>, e: PE<T>, t: T, x: X }

    public enum NPE<T> {
        One { s: PS<T>, q: PE<T>, x: X , t: T},
        Two(PS<T>, PE<T>, X , T),
        Three,
    }
}
