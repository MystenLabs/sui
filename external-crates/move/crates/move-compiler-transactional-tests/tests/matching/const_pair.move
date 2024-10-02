//# init --edition 2024.beta

//# publish
module 0x42::m {

    const V: vector<u64> = vector[1,2,3];

    public struct Pair<T>(T, T) has drop;

    public fun test(p: &Pair<vector<u64>>): bool {
        match (p) {
            Pair(V, V) => true,
            _ => false,
        }
    }

    public fun make_pair(v1: vector<u64>, v2: vector<u64>): Pair<vector<u64>> {
        Pair(v1, v2)
    }

}

//# run
module 0x43::main {

    use 0x42::m;

    fun main() {
        let v0 = vector[0,0];
        let v1 = vector[1,2,3];
        let v2 = vector[1,3,2];

        assert!(!m::test(&m::make_pair(v0, v0)), 0);
        assert!(!m::test(&m::make_pair(v0, v1)), 1);
        assert!(!m::test(&m::make_pair(v0, v2)), 2);
        assert!(!m::test(&m::make_pair(v1, v0)), 3);
        assert!(!m::test(&m::make_pair(v1, v2)), 4);
        assert!(!m::test(&m::make_pair(v2, v0)), 5);
        assert!(!m::test(&m::make_pair(v2, v1)), 6);
        assert!(!m::test(&m::make_pair(v2, v2)), 7);

        assert!(m::test(&m::make_pair(v1, v1)), 8);

    }

}
