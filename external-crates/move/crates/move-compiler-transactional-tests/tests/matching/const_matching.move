//# init --edition 2024.beta

//# publish
module 0x42::m {

    const Z: u64 = 0;
    const SZ: u64 = 1;
    const SSZ: u64 = 2;

    const Z8: u8 = 0;
    const SZ8: u8 = 1;
    const SSZ8: u8 = 2;

    public fun t00(n: u64): u64 {
        match (n) {
            Z => 0,
            SZ => 1,
            SSZ => n - 1,
            _ => n
        }
    }

    public fun t01(n: u8): u8 {
        match (n) {
            Z8 => 0,
            SZ8 => 1,
            SSZ8 => n - 1,
            _ => n
        }
    }

    public fun t02(n: &u64): u64 {
        match (n) {
            Z => 0,
            SZ => 1,
            SSZ => *n - 1,
            _ => *n
        }
    }

    public fun t03(n: &u8): u8 {
        match (n) {
            Z8 => 0,
            SZ8 => 1,
            SSZ8 => *n - 1,
            _ => *n
        }
    }

    public fun t04(n: &mut u64): u64 {
        match (n) {
            Z => 0,
            SZ => 1,
            SSZ => *n - 1,
            _ => *n
        }
    }

    public fun t05(n: &mut u8): u8 {
        match (n) {
            Z8 => 0,
            SZ8 => 1,
            SSZ8 => *n - 1,
            _ => *n
        }
    }

}

//# run
module 0x43::main {
    use 0x42::m::{t00, t01, t02, t03, t04, t05};

    fun main() {

        let mut i = 0;

        while (i < 10) {
            if (i == 2) {
                assert!(t00(i) == 1, i);
                assert!(t02(&i) == 1, i + 20 );
                assert!(t04(&mut i) == 1, i + 40);
                let mut i8: u8 = i as u8;
                assert!(t01(i8) == 1, i + 10);
                assert!(t03(&i8) == 1, i + 30);
                assert!(t05(&mut i8) == 1, i + 50);
            } else {
                assert!(t00(i) == i, i);
                assert!(t02(&i) == i, i + 20);
                assert!(t04(&mut i) == i, i + 40);
                let mut i8: u8 = i as u8;
                assert!(t01(i8) == i8, i + 10);
                assert!(t03(&i8) == i8, i + 30);
                assert!(t05(&mut i8) == i8, i + 50);
            };
            i = i + 1;
        }

    }

}
