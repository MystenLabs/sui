//  This specific test will improve in the borrow checker rewrite
module 0x42::m;

public fun f(test: bool): &u64 {
    let (a, b, c, d, e, f, g, h, i, j, k) = (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);
    let mut x = &a;
    if (test) x = &b;
    if (test) x = &c;
    if (test) x = &d;
    if (test) x = &e;
    if (test) x = &f;
    if (test) x = &g;
    if (test) x = &h;
    if (test) x = &i;
    if (test) x = &j;
    if (test) x = &k;
    test && test;
    x
}
