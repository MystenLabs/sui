module a::m {
    macro fun call<$T>($f: || -> $T): $T {
        $f()
    }

    fun commands(cond: bool) {
        call!(|| {
            if (cond) return 0u64;
            if (cond) return false;
            return @0
        });
        call!(|| {
            if (cond) return &0u64;
            if (cond) return &mut 0;
            return 0u64
        });
        call!(|| {
            if (cond) return (&0u64, vector[0u64]);
            if (cond) return (&mut 0u64, vector[false]);
            return (&0u64, vector[])
        });
        call!(|| {
            if (cond) return (&0u64, vector[0u64]);
            if (cond) return (&0u64, vector[0u64], 1u64);
            return (&0u64, vector[0u64])
        });
    }

}
