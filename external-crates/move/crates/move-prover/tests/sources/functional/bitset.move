module 0x42::bitset128 {
    struct BitSet128 has drop {
        s: u128
    }

    spec BitSet128 {
        pragma bv=b"0";
    }

    public fun empty(): BitSet128 {
        BitSet128 {
            s: 0
        }
    }

    public fun insert(s: &mut BitSet128, i: u64) {
        assert!(i < 128, 0);
        s.s = s.s | (1 << (i as u8));
    }


    public fun remove(s: &mut BitSet128, i: u64) {
        assert!(i < 128, 0);
        s.s = s.s & (0xffff ^ (1 << (i as u8)));
    }

    public fun contains(s: &BitSet128, i: u64): bool {
        assert!(i < 128, 0);
        (s.s & (1 << (i as u8))) > 0
    }

    public fun union(s1: &BitSet128, s2: &BitSet128): BitSet128 {
        BitSet128 {
            s: s1.s | s2.s
        }
    }

    public fun intersect(s1: &BitSet128, s2: &BitSet128): BitSet128 {
        BitSet128 {
            s: s1.s & s2.s
        }
    }


    public fun complement(s: &BitSet128): BitSet128 {
        BitSet128 {
            s: (0xffffffffffffffffffffffffffffffff ^ s.s)
        }
    }

    public fun difference(s1: &BitSet128, s2: &BitSet128): BitSet128 {
        BitSet128 {
            s: s1.s & complement(s2).s
        }
    }

    #[verify_only]
    fun ident(s: &BitSet128) {
        let s2 = intersect(s, s);
        let s3 = union(s, s);
        //let s4 = complement(&complement(s));

        spec {
            assert s == s2;
            assert s == s3;
            //assert s == s4; // timeout
        };
    }

    /*
    // timeout
    #[verify_only]
    fun de_morgan_1(s1: &BitSet128, s2: &BitSet128) {
        let s3 = complement(&intersect(s1, s2));
        let s4 = union(&complement(s1), &complement(s2));
        spec {
            assert s3 == s4;
        };
    }
    */

    #[verify_only]
    fun de_morgan_2(s1: &BitSet128, s2: &BitSet128) {
        let s3 = complement(&union(s1, s2));
        let s4 = intersect(&complement(s1), &complement(s2));
        spec {
            assert s3 == s4;
        };
    }

}
