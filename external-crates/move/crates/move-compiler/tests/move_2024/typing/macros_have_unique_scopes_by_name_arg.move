module a::m {
    // macros have their own unique scopes
    macro fun foo($x: u64, $f: address): (u64, bool, address) {
        let x = $x;
        $f; // try to capture x
        let a = $f;
        (x, { let x = false; x }, a)
    }

    macro fun bar($x: vector<u64>, $f: address): (u64, bool, address) {
        let x = $x;
        $f; // try to capture x
        let x = get(x);
        let a = $f;
        foo!(x, { let x = a; x })
    }

    fun t() {
        foo!(0, @0);
        foo!(0, { let x = @0; x });
        foo!(0, { let (_, _, x) = foo!(0, { let x = @0; x }); x });
        bar!(vector[0], { let x = vector[@0]; get(x) });
        bar!(vector[0], { let x = vector[@0]; let x = get(x); x });
        bar!(
            vector[0],
            {
                let x = vector[vector[@0]];
                let (_, _, x) = bar!(
                    vector[0],
                    { let x = get(x); let x = get(x); x },
                );
                x
            },
        );
    }

    fun get<T: copy + drop>(_: vector<T>): T { abort 0 }
}
