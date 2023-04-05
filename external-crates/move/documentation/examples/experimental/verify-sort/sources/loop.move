// exclude_for: cvc5
module 0x42::VerifySort {

    use std::vector;
    // iperm is a ghost variable for verification
    public fun verify_sort(v: &mut vector<u64>, iperm: &mut vector<u64>) {
        let vlen = vector::length(v);
        spec {
            assume vlen == 42;
            assume len(iperm) == vlen && (forall k in 0..vlen : iperm[k] == k);
        };
        if (vlen <= 1) return ();

        let i = 0;
        let j = 1;
        while
        ({
            spec {
                // loop invariant that proves order of output vector
                invariant i >= 0 && i < vlen;
                invariant j >= 1 && j <= vlen;
                invariant j == vlen ==> i == vlen - 1;
                invariant j >= i + 1;
                invariant len(v) == vlen;
                invariant forall k in 0..i, l in 0..i : k < l ==> v[k] <= v[l];
                invariant forall k in 0..i, l in i..vlen : v[k] <= v[l];
                invariant forall k in (i + 1)..j : v[i] <= v[k];

                // loop invariant that proves output vector is a permutation of input vector
                invariant len(iperm) == vlen &&
                          (forall k in 0..vlen : old(v)[iperm[k]] == v[k]) &&
                          (forall k in 0..vlen : iperm[k] >= 0 && iperm[k] < vlen) &&
                          (forall k in 0..vlen, l in 0..vlen : k != l ==> iperm[k] != iperm[l]);
            };
            (i < vlen - 1)
        })
        {
            if (*vector::borrow(v, i) > *vector::borrow(v, j)) {
                vector::swap(v, i, j);
                vector::swap(iperm, i, j);
            };

            if (j < vlen - 1 ) {
                j = j + 1;
            } else {
                i = i + 1;
                j = i + 1;
            };
        };
        spec {
            assert len(v) == vlen;
            assert i == vlen - 1;
            assert j == vlen;
            assert v[0] <= v[1];
            assert v[vlen - 2] <= v[vlen - 1];
        };
    }
    spec verify_sort {
        aborts_if false;
        ensures forall i in 0..len(v)-1: v[i] <= v[i+1];
        ensures exists perm : vector<u64> : len(perm) == len(v) &&
                (forall k in 0..len(v) : old(v)[perm[k]] == v[k]) &&
                (forall k in 0..len(v) : perm[k] >= 0 && perm[k] < len(v)) &&
                (forall k in 0..len(v), l in 0..len(v) : k != l ==> perm[k] != perm[l]);
    }
}
