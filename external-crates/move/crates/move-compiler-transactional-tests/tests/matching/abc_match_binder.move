//# init --edition 2024.alpha

//# publish
module 0x42::m {

    public enum ABC<T> has drop {
        A(T),
        B,
        C(T)
    }

    fun t0(x: ABC<u64>): ABC<u64> {
        match (x) {
            ABC::C(c) => ABC::C(c),
            ABC::A(a) => ABC::A(a),
            y => y,
        }
    }

}
