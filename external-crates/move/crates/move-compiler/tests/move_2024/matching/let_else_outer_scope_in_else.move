// The `else` branch has access to the enclosing scope; binders introduced
// earlier in the function are visible inside the else block.
module 0x42::m {

    public enum ABC<T> has drop {
        A(T),
        B,
        C(T),
    }

    fun outer_in_else(b: ABC<u64>): u64 {
        let fallback = 99u64;
        let ABC::C(x) = b else { return fallback };
        x
    }

}
