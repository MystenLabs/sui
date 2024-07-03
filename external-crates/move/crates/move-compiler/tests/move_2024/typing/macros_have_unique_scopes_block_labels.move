#[allow(dead_code)]
module a::blocks {
    // macros have their own unique scopes
    macro fun foo($x: u64, $f: |u64| -> u64): (u64, bool) {
        'a: {
            $f($x); // try to capture a
            let x = $f(return 'a (0, false));
            (x, true)
        }
    }

    macro fun bar<$T>($x: vector<u64>, $f: |vector<u64>| -> u64): (u64, bool) {
        'a: {
            $f($x); // try to capture a
            let x = vector[$f(return 'a (0, false))];
            foo!(0, |_| 'a: { return'a get(x) })
        }
    }

    fun t() {
        foo!(0, |x| x);
        foo!(0, |x| 'a: { return'a x });
        foo!(0, |x| 'a: { let (i, _) = foo!(0, |y| return'a x + y); i });
        bar!(vector[0], |x| get(x));
        bar!(vector[0], |x| 'a: { let x = get(x); return'a x });
        bar!(
            vector[0],
            |x| 'a: { let (i, _) = bar!(return'a get(x), |x| 'a: { let x = get(x); return'a x }); i },
        );
    }

    fun get<T: copy + drop>(_: vector<T>): T { abort 0 }
}

#[allow(dead_code)]
module a::loops {
    // macros have their own unique scopes
    macro fun foo($x: u64, $f: |u64| -> u64): (u64, bool) {
        'a: loop {
            $f($x); // try to capture a
            let _x = $f(break 'a (0, false));
        }
    }

    macro fun bar<$T>($x: vector<u64>, $f: |vector<u64>| -> u64): (u64, bool) {
        'a: loop {
            $f($x); // try to capture a
            let x = vector[$f(break 'a (0, false))];
            foo!(0, |_| { break'a (get(x), true) });
        }
    }

    fun t() {
        foo!(0, |x| x);
        foo!(0, |x| 'a: { return'a x });
        foo!(0, |x| 'a: { let (i, _) = foo!(0, |y| return'a x + y); i });
        bar!(vector[0], |x| get(x));
        bar!(vector[0], |x| 'a: { let x = get(x); return'a x });
        bar!(
            vector[0],
            |x| 'a: { let (i, _) = bar!(return'a get(x), |x| 'a: { let x = get(x); return'a x }); i },
        );
    }

    fun get<T: copy + drop>(_: vector<T>): T { abort 0 }
}



#[allow(dead_code)]
module a::whiles {
    // macros have their own unique scopes
    macro fun foo($x: u64, $f: |u64| -> u64): (u64, bool) {
        'a: while (true) {
            $f($x); // try to capture a
            let _x = $f(break 'a);
        };
        (0, false)
    }

    macro fun bar<$T>($x: vector<u64>, $f: |vector<u64>| -> u64): (u64, bool) {
        'a: while (true) {
            $f($x); // try to capture a
            let _x = vector[$f(break 'a)];
            foo!(0, |_| { break'a });
        };
        (0, false)
    }

    fun t() {
        foo!(0, |x| x);
        foo!(0, |x| 'a: { return'a x });
        foo!(0, |x| 'a: { let (i, _) = foo!(0, |y| return'a x + y); i });
        bar!(vector[0], |x| get(x));
        bar!(vector[0], |x| 'a: { let x = get(x); return'a x });
        bar!(
            vector[0],
            |x| 'a: { let (i, _) = bar!(return'a get(x), |x| 'a: { let x = get(x); return'a x }); i },
        );
    }

    fun get<T: copy + drop>(_: vector<T>): T { abort 0 }
}
