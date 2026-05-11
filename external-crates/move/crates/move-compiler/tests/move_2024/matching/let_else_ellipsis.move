// Field-pattern and positional-pattern ellipsis (`..`) routes through
// `match_pattern`, which `let ... else` uses directly. Pins both forms.
module 0x42::m {

    public enum FE<T> has drop {
        S { x: T, y: T, z: T },
        N,
    }

    public enum PE<T> has drop {
        S(T, T, T),
        N,
    }

    fun field_ellipsis(e: FE<u64>): u64 {
        let FE::S { x, .. } = e else { return 0 };
        x
    }

    fun positional_ellipsis(e: PE<u64>): u64 {
        let PE::S(x, ..) = e else { return 0 };
        x
    }

}
