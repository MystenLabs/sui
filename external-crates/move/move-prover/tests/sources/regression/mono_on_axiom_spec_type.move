module 0x42::mono_on_axiom_spec_type {
    struct SetByVec {
        elems: vector<u8>,
    }
    spec SetByVec {
        invariant forall i in 0..len(elems), j in 0..len(elems):
            elems[i] == elems[j] ==> i == j;
    }

    fun new_set(): SetByVec {
        SetByVec {
            elems: std::vector::empty(),
        }
    }

    spec module {
        fun deserialize<T>(bytes: vector<u8>): T;

        axiom<T> forall b1: vector<u8>, b2: vector<u8>:
            (deserialize<T>(b1) == deserialize<T>(b2) ==> b1 == b2);
    }
}
