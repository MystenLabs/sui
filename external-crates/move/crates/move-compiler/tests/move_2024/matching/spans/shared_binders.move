module 0x42::m {

    public enum E has drop {
        A { n: u64 },
    }

    // Both arms match the same variant, so they share one unpack. The binders it
    // produces must not be attributed to whichever arm happened to build them.
    fun shared(e: &E): u64 {
        match (e) {
            E::A { n: first } if (*first > 0) => *first,
            E::A { n: second } => *second,
        }
    }

}
