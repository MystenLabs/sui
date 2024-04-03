module 0x42::m {

    public enum Option<T> has drop {
        None,
        Some(T)
    }

    fun foo<T: drop>(_x: T): () {
    }

    fun t0(): u64 {
        let o = Option::None;
        match (o) {
            Option::Some(n) => foo(n),
            Option::None => (),
        };
        let _o = Option::Some(0);
        0
    }
}
