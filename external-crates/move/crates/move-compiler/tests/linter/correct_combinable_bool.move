module 0x42::M {
    const ERROR_NUM: u64 = 2;
    
    public fun func1(x: u64, y: u64, z: u64) {
        if (x == y || z < y) {};
        if (x <= y) {};
        if (x >= y) {};
        if (x > y) {};
        if (x < y) {};
        if (x == 11 || x < 3) {};
    }
}
