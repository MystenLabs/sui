module a::m {
    macro fun do<$T>($f: || -> $T): $T { $f() }

    // lambdas capture break/continue
    fun t() {
        do!(|| {
            if (false) break;
            if (false) continue;
        });
    }

    fun tloop() {
        loop {
            do!(|| {
                if (false) break;
                if (false) continue;
            });
        };

        while (true) {
            do!(|| {
                if (false) break;
                if (false) continue;
            });
        }
    }

    fun tnamedloop() {
        'a: loop {
            do!(|| {
                if (false) break;
                if (false) continue;
            });
        };

        'b: while (true) {
            do!(|| {
                if (false) break;
                if (false) continue;
            });
        }
    }

}
