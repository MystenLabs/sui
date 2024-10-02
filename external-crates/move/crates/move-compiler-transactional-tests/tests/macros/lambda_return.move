//# init --edition 2024.alpha

//# publish

#[allow(dead_code)]
module 42::m {
    macro fun foo($x: || -> u64): u64 {
       $x() + $x()
    }

    fun t(cond: bool): vector<u64> {
        vector[
            foo!(|| {
                if (cond) return 1;
                0
            }),
            foo!(|| {
                if (!cond) return 2;
                4
            }),
            foo!(|| return 8),
        ]
    }
}

//# run 42::m::t --args true

//# run 42::m::t --args false
