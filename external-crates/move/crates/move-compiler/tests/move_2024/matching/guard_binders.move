module 0x42::m {

    public enum Option<T> has drop {
        None,
        Some(T)
    }

    fun imm_default<T>(_o: &Option<T>): u64 {
        0
    }

    fun mut_default<T>(_o: &mut Option<T>): u64 {
        0
    }

    fun t0(): u64 {
        let o: Option<u64> = Option::None;
        match (&o) {
            Option::Some(n) if n == &5 => *n,
            Option::None => 3,
            z => imm_default(z),
        }
    }

    fun t1(): u64 {
        let mut o: Option<u64> = Option::None;
        match (&mut o) {
            Option::Some(n) if n == &5 => *n,
            Option::None => 3,
            z => mut_default(z),
        }
    }

}
