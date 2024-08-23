module 0x42::m {

    public struct S(u64, u64)

    fun t(s: &S): u64 {
        match (s) {
            S(_, _x @ _) => 10
        }
    }
}
