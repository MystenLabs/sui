module 0x1::ReturnRefsSafe {
    // Make sure the analysis doesn't complain about returning
    // refs to formals or their children

    fun return_immut(x: &u64): &u64 {
        x
    }

    fun return_mut(x: &mut u64): &mut u64 {
        x
    }

    fun return_freeze(x: &mut u64): &u64 {
        x
    }

    fun return_vec_immut(v: &vector<u64>): &vector<u64> {
        v
    }

    fun return_vec_mut(v: &mut vector<u64>): &mut vector<u64> {
        v
    }

}
