module 0x42::m {

    const V: vector<u64> = vector[1,2,3];

    public struct Pair<T>(T, T)

    fun test(p: &Pair<vector<u64>>): bool {
        match (p) {
            Pair(V, V) => true,
            _ => false,
        }
    }

}
