module a::m {
    // macros have their own unique scopes
    macro fun foo($x: u64, $f: || -> address): (u64, bool, address) {
        let x = $x;
        $f(); // try to capture x
        let a = $f();
        (x, { let x = false; x }, a)
    }

    macro fun bar<$T>($x: vector<u64>, $f: |$T| -> address, $v: $T): (u64, bool, address) {
        let x = $x;
        $f($v); // try to capture x
        let x = get(x);
        let a = $f($v);
        foo!(x, || { let x = a; x })
    }

    fun t() {
        foo!(0, || @0);
        foo!(0, || { let x = @0; x });
        foo!(0, || { let (_, _, x) = foo!(0, || { let x = @0; x }); x });
        bar!(vector[0], |x| get(x), vector[@0]);
        bar!(vector[0], |x| { let x = get(x); x }, vector[@0]);
        bar!(
            vector[0],
            |x| { let (_, _, x) = bar!(vector[0], |x| { let x = get(x); x }, get(x)); x },
            vector[vector[@0]],
        );
    }

    fun get<T: copy + drop>(_: vector<T>): T { abort 0 }
}
