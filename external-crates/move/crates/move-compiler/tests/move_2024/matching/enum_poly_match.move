module 0x42::m {

    public enum Threes<T> has drop {
        Zero,
        One(T),
        Two(T, T),
        Three(T, T, T)
    }

    fun t0(): u64 {
        match (Threes::One(0)) {
            Threes::Three(x, _, _) => x,
            Threes::One(x) => x,
            Threes::Two(_, _) => 1,
            Threes::Zero => 64,
        }
    }

}
