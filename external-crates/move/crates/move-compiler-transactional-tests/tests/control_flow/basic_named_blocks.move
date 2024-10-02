//# init --edition 2024.alpha

//# publish
module 0x42::m {

    #[allow(dead_code)]
    public fun t00(): u64 {
        'name: {
            return 'name 10;
            20
        }
    }

    #[allow(dead_code)]
    public fun t01(): u64 {
        'name: {
            'name2: {
                return 'name 10;
                20
            }
        }
    }

    #[allow(dead_code)]
    public fun t02(): u64 {
        'outer: loop {
            let _x = 'inner: loop {
                break 'outer 10;
                break 'inner 20
            };
        }
    }

    #[allow(dead_code)]
    public fun t03(): u64 {
        'outer: loop {
            let x = 'inner: loop {
                break 'inner 10
            };
            break 'outer x
        }
    }

    public fun t04(cond: bool): u64 {
        'name: while (cond) {
            break 'name
        };
        10
    }

    public fun t05(cond: bool): u64 {
        'outer: loop {
            'body: while (cond) {
                if (cond) { break 'outer 10 };
                continue 'body
            }
        }
    }

    public fun t06(cond: bool): u64 {
        'name: while (cond) {
            return 10
        };
        20
    }

}

//# run
module 0x43::main {
use 0x42::m;
fun main() {
    assert!(m::t00() == 10, 0);
    assert!(m::t01() == 10, 1);
    assert!(m::t02() == 10, 2);
    assert!(m::t03() == 10, 3);
    assert!(m::t04(true) == 10, 4);
    assert!(m::t05(true) == 10, 5);
    assert!(m::t06(true) == 10, 6);
}
}
