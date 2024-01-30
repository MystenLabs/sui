module a::m {
    macro fun call<$T>($f: || -> $T): $T {
        $f()
    }

    fun commands(cond: bool) {
        call!(|| {
            if (cond) return 0;
            if (cond) return false;
            return @0
        });
        call!(|| {
            if (cond) return &0;
            if (cond) return &mut 0;
            return 0
        });
        call!(|| {
            if (cond) return (&0, vector[0]);
            if (cond) return (&mut 0, vector[false]);
            return (&0, vector[])
        });
        call!(|| {
            if (cond) return (&0, vector[0]);
            if (cond) return (&0, vector[0], 1);
            return (&0, vector[0])
        });
    }

}
