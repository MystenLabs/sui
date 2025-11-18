module a::m {
    macro fun call<$T>($f: || -> $T): $T {
        $f()
    }

    fun conditional(cond: bool) {
        call!(|| { if (cond) return 0u64; &1u64 });
        call!(|| { if (cond) 1u64 else return &0u64 });
        call!(|| { if (cond) return 0u64; &1u64 });
        call!(|| { if (cond) 1u64 else return &0u64 });
        call!(|| { if (cond) return (vector[], 0u64, false); (vector[0u64], true) });
        call!(|| { if (cond) (vector[], 0u64, false) else return (vector[0u64], true) });
    }
}
