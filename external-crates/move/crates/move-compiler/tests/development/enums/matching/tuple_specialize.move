module 0x42::m {

    public enum Tup<A,B,C> {
        Triple(A, B, C)
    }

    public fun match_tup(t: &Tup<u64, u64, u64>): u64 {
        match (t) {
            Tup::Triple(x, y, 10) => 10 + *x + *y,
            Tup::Triple(x, 5, 10) => 15 + *x,
            Tup::Triple(5, 5, 10) => 20,
            Tup::Triple(5, y, z) => 5 + *y + *z,
            Tup::Triple(5, 5, z) => 10 + *z,
            Tup::Triple(x, y, z) => *x + *y + *z,
        }
    }

}
