module a::m {
    const Z: u64 = 0;
    const SZ: u64 = 1;
    const SSZ: u64 = 2;

    const Z8: u8 = 0;
    const SZ8: u8 = 1;
    const SSZ8: u8 = 2;

    fun t00(n: u64): u64 {
        match (n) {
            Z => 0,
            SZ => 1,
            SSZ => n - 1,
            _ => n
        }
    }

    fun t01(n: u8): u8 {
        match (n) {
            Z8 => 0,
            SZ8 => 1,
            SSZ8 => n - 1,
            _ => n
        }
    }

    fun t02(n: &u64): u64 {
        match (n) {
            Z => 0,
            SZ => 1,
            SSZ => *n - 1,
            _ => *n
        }
    }

    fun t03(n: &u8): u8 {
        match (n) {
            Z8 => 0,
            SZ8 => 1,
            SSZ8 => *n - 1,
            _ => *n
        }
    }

    fun t04(n: &mut u64): u64 {
        match (n) {
            Z => 0,
            SZ => 1,
            SSZ => *n - 1,
            _ => *n
        }
    }

    fun t05(n: &mut u8): u8 {
        match (n) {
            Z8 => 0,
            SZ8 => 1,
            SSZ8 => *n - 1,
            _ => *n
        }
    }

}
