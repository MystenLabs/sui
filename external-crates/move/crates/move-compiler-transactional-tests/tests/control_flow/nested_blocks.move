//# init --edition 2024.alpha

//# publish
module 0x42::m {

    #[allow(dead_code)]
    public fun t00(): u64 {
        if ('name: { return 'name 10 } == 10) {
            'outer: loop {
                return 10
            }
        } else {
            return 20
        }
    }

    #[allow(dead_code)]
    public fun t01(): u64 {
        if ('name:  { return 'name 20 } == 10) {
            'outer: loop {
                return 20
            }
        };
        return ('name: {
            'inner: loop {
                return 'name 10
            }
        });
        20
    }

    #[allow(dead_code)]
    public fun t02(): u64 {
        let x = 'outer: loop {
            if ('name: { return 'name 20 } == 10) {
                'outer: loop {
                    return 20
                }
            };
            break 'outer 10
        };
        while (x != x) {
            return 20
        };
        x
    }

    #[allow(dead_code)]
    public fun t03(): u64 {
        let x = 'outer: loop {
            if ('name:  { return 'name 20 } == 10) {
                'outer: loop {
                    return 20
                }
            };
            break 'outer 20
        };
        while (x == x) {
            return 10
        };
        x
    }

    #[allow(dead_code)]
    public fun t04(): u64 {
        let a = 'all: {
            loop {
                let x = 'outer: loop {
                    if ('name:  { return 'name 20 } == 10) {
                        'outer: loop {
                            return 20
                        }
                    };
                    break 'outer 20
                };
                while (x == x) {
                    return 'all 10
                }
            };
            20
        };
        a
    }

    #[allow(dead_code)]
    public fun t05(): u64 {
        let a = 'all: {
            loop {
                let x = 'outer: loop {
                    if ('name:  { return 'name 20 } == 10) {
                        'outer: loop {
                            return 20
                        }
                    };
                    break 'outer 20
                };
                while (x == x) {
                    return 10
                }
            };
            20
        };
        a
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
    assert!(m::t04() == 10, 4);
    assert!(m::t05() == 10, 5);
}
}
