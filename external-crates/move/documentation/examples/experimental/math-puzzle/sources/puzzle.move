module 0x42::Puzzle {
    fun assert0(b: bool) {
        assert!(b, 0);
    }

    fun puzzle(a: u64, b: u64, c: u64, d: u64, e: u64, f: u64, g: u64, h: u64) {
        assert0(1 <= a && a <= 9);         // 1 <= a <= 9
        assert0(1 <= b && b <= 9);         // 1 <= b <= 9
        assert0(1 <= c && c <= 9);         // 1 <= c <= 9
        assert0(1 <= d && d <= 9);         // 1 <= d <= 9
        assert0(1 <= e && e <= 9);         // 1 <= e <= 9
        assert0(1 <= f && f <= 9);         // 1 <= f <= 9
        assert0(1 <= g && g <= 9);         // 1 <= g <= 9
        assert0(1 <= h && h <= 9);         // 1 <= h <= 9

        assert0(a == c*2);                 // a is the double of c
        assert0(b < h);                    // b is less than h
        assert0(c == e);                   // c is equal to e
        assert0(d == f);                   // d is equal to f
        assert0(e <= 3);                   // e is less than or equal to 3
        assert0(f % 2 == 1);               // f is odd
        assert0(g % 2 == 0);               // g is even
        assert0(h >= 5);                   // h is greater than or equal to 5

        assert0((c+e)%10 == h);            //      a b c
        let carry = (c+e)/10;              //    +   d e
        assert0((b+d+carry)%10 == g);      //    -------
        assert0(a+(b+d+carry)/10 == f);    //      f g h
    }
    spec puzzle {
        aborts_if true;                    // specifies that this fun always aborts.
    }
}
