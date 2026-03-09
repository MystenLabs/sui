module 0x0::MRE {
    public enum A has drop {
        X,
    }

    public enum B has drop {
        Y,
    }

    fun f(a: A): u64 {
        match (a) {
            A::X | B::Y => 1,
        }
    }
}
