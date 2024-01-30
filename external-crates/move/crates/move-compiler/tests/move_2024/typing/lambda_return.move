module a::m {
    macro fun call<$T>($f: || -> $T): $T {
        $f()
    }

    #[allow(dead_code)]
    fun simple() {
        call!<u64>(|| return 0);
        call!<&u64>(|| return &0);
        call!<(&u64, u8)>(|| return (&0, 1));
    }

    fun conditional(cond: bool) {
        call!<u64>(|| { if (cond) return 0; 1 });
        call!<u64>(|| { if (cond) 1 else return 0 });
        call!<&u64>(|| { if (cond) return &0; &1 });
        call!<&u64>(|| { if (cond) &1 else return &0 });
        call!<(vector<u64>, bool)>(|| { if (cond) return (vector[], false); (vector[0], true) });
        call!<(vector<u64>, bool)>(|| { if (cond) (vector[], false) else return (vector[0], true) });
    }

    #[allow(dead_code)]
    fun commands(cond: bool) {
        call!(|| {
            if (cond) return false;
            if (cond) return false;
            return true
        });
        call!(|| {
            if (cond) return &0;
            if (cond) return &mut 0;
            return &0
        });
        call!(|| {
            if (cond) return (&0, vector[0]);
            if (cond) return (&mut 0, vector[0, 1]);
            return (&0, vector[])
        });
    }
}
