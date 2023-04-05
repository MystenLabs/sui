module 0x42::TestAxioms {

    spec module {
        pragma verify = true;
    }

    // ----------------------------------------------------
    // Non-generic axiom

    spec module {
        fun spec_incr(x: num): num;
        axiom forall x: num: spec_incr(x) == x + 1;
    }

    fun incr(x: u64): u64 {
        x + 1
    }
    spec incr {
        ensures result == TRACE(spec_incr(x));
    }

    // ----------------------------------------------------
    // Generic axiom

    spec module {
        fun spec_id<T>(x: T): T;
        axiom<T> forall x: T: spec_id(x) == x;
    }

    fun id_T<T>(x: T): T {
        x
    }
    spec id_T {
        ensures result == spec_id(x);
    }

    fun id_u64<T>(x: u64): u64 {
        x
    }
    spec id_u64 {
        ensures result == spec_id(x);
    }

    // ----------------------------------------------------
    // Generic axiom calling spec function

    spec module {
        use std::bcs::serialize;
        fun deserialize<T>(bytes: vector<u8>): T;
        // We need the trigger annotation below, otherwise timeout
        axiom<T> forall v: T {serialize(v)}: deserialize<T>(serialize(v)) == v;
    }
}
