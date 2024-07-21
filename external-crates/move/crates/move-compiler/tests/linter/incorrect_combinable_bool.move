module 0x42::M {
    const ERROR_NUM: u64 = 2;
    public fun func1(x: u64, y: u64) {
        let m = 3;
        let n = 4;
        if (x < y || x == y) {}; // should be x <= y
        if (x == y || x > y) {}; // should be x >= y
        if (x > y || x == y) {}; // should be x >= y
        if (m == n || m < n) {}; // should be m <= n
        if (x == 11 || x < 11) {};
    }
}
