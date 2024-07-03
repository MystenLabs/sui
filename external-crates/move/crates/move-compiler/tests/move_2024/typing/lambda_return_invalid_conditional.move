module a::m {
    macro fun call<$T>($f: || -> $T): $T {
        $f()
    }

    fun conditional(cond: bool) {
        call!(|| { if (cond) return 0; &1 });
        call!(|| { if (cond) 1 else return &0 });
        call!(|| { if (cond) return 0; &1 });
        call!(|| { if (cond) 1 else return &0 });
        call!(|| { if (cond) return (vector[], 0, false); (vector[0], true) });
        call!(|| { if (cond) (vector[], 0, false) else return (vector[0], true) });
    }
}
