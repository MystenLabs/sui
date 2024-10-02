//# init --edition 2024.alpha

//# publish

module 42::m {
    macro fun foo($x: u64): u64 {
        $x + $x
    }

    fun t(cond: bool) {
        let res = foo!('a: {
            if (cond) return'a 1;
            0
        });
        assert!(res == 2, 0);
        let res = foo!('a: {
            if (!cond) return'a 2;
            4
        });
        assert!(res == 8, 0);
    }
}

//# run 42::m::t --args true

// should abort
//# run 42::m::t --args false
