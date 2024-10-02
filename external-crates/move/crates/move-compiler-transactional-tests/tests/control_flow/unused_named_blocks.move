//# init --edition 2024.alpha

//# publish
module 0x42::m {

    public fun t0(): u64 {
        if ('block: { true }) {
            10
        } else {
            20
        }
    }

    public fun t1(): u64 {
        if ('block: { true }) {
            'block: { 10 }
        } else {
            20
        }
    }

    public fun t2(): u64 {
        if ('block: { false }) {
            20
        } else {
            'block: { 10 }
        }
    }

    public fun t3(): u64 {
        if ('block: { false }) {
            20
        } else {
            while (false) { 'block: { 20 }; };
            'block: { 10 }
        }
    }

    public fun t4(): u64 {
        let mut count = 0;
        let mut x = 0;
        while (x < 10) {
            'inner: {
                count = count + 1;
            };
            x = x + 1;
        };
        count
    }

    public fun t5(): u64 {
        let mut count = 0;
        let (start, stop) = (0, 10);
        let mut i = start;
        while (i < stop) {
            let x = i;
            count = count + x * x;
            i = i + 1;
        };
        count
    }

    public fun t6(): u64 {
        let mut count = 0u64;
            {
            let (start, stop) = (0, 10);
            'macro:  {
                'lambdabreak:  {
                        {
                        let mut i = start;
                        while (i < stop) 'loop: {
                                {
                                let x = i;
                                'lambdareturn:  {
                                    count = count + x * x;
                                }
                            };
                            i = i + 1;
                        }
                    }
                }
            }
        };
        count
    }
}

//# run
module 0x43::main {
use 0x42::m;
fun main() {
    assert!(m::t0() == 10, 0);
    assert!(m::t1() == 10, 1);
    assert!(m::t2() == 10, 2);
    assert!(m::t3() == 10, 3);
    assert!(m::t4() == 10, 4);
    assert!(m::t5() == 285, 5);
    assert!(m::t6() == 285, 6);
}
}
