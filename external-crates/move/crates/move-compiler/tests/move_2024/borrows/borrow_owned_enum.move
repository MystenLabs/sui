module a::m;

public enum E has drop {
    BigVariant(u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64),
}

public fun test_mut_borrow(e: E, test: bool): &u64 {
    let e = e;
    let mut x;
    let y = 0;
    let (a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p) = match (&e) {
        E::BigVariant(a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p) => {
            (a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p)
        }
    };

    x = &y;
    if (test) x = a;
    if (test) x = b;
    if (test) x = c;
    if (test) x = d;
    if (test) x = e;
    if (test) x = f;
    if (test) x = g;
    if (test) x = h;
    if (test) x = i;
    if (test) x = j;
    if (test) x = k;
    if (test) x = l;
    if (test) x = m;
    if (test) x = n;
    if (test) x = o;
    if (test) x = p;

    x
}
