module a::m {
    macro fun do<$T>($f: || -> $T): $T { $f() }
    macro fun do2<$T1, $T2>($f: || -> $T1, $g: || -> $T2): ($T1, $T2) { ($f(), $g()) }


    // simple test of break/return in a lambda with a named block
    fun t() {
        do!(|| 'a: {
            if (false) return'a 0;
            0
        });
        do!(|| ('a: {
            if (false) return'a 0;
            0
        }));
        do2!(|| 'a: {
            if (false) return'a 0;
            0
        },
        || 'b: {
            if (false) return'b 0;
            0
        });
    }
    fun nested() {
        do!(|| 'outer: {
            do2!(|| 'a: {
                if (false) return'outer (0, 1);
                if (false) return'a 0;
                0
            },
            || 'b: {
                if (false) return'outer (0, 1);
                if (false) return'b 0;
                0
            })
        });
    }
}
