//# init --edition 2024.beta

//# publish
module 0x42::m {

    public enum E has drop {
        A { a: vector<u64> },
        B { b: vector<u64> }
    }

    fun get_vec(e: &E): &vector<u64> {
        match (e) {
            E::A { a } => a,
            E::B { b } => b,
        }
    }

    #[syntax(index)]
    fun e_index(e: &E, ndx: u64): &u64 {
        &e.get_vec()[ndx]
    }

    fun test() {
        let e = E::A { a: vector[0,1,2,3,4] };
        assert!(e[0] == 0);
        assert!(e[1] == 1);
        assert!(e[2] == 2);
        assert!(e[3] == 3);
        assert!(e[4] == 4);
        let e = E::B { b: vector[0,1,2,3,4] };
        assert!(e[0] == 0);
        assert!(e[1] == 1);
        assert!(e[2] == 2);
        assert!(e[3] == 3);
        assert!(e[4] == 4);
    }
}

//# run 0x42::m::test
