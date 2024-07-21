module 0xbadbadbad::BasicCoin {
    struct Coin<phantom T0> has store {
        value: u64,
    }

    struct Coin2<phantom T0> has store {
        value: u64,
        value2: u64,
    }

    struct Balance<phantom T0> has key {
        coin: Coin<T0>,
    }

    struct R has copy, drop {
        x: u64,
    }

    fun pop_smallest_while_not_equal(arg0: vector<u64>, arg1: vector<u64>) : vector<u64> {
        let v0 = 0x1::vector::empty<u64>();
        while (!0x1::vector::is_empty<u64>(&arg0) && !0x1::vector::is_empty<u64>(&arg1)) {
            let v1 = *0x1::vector::borrow<u64>(&arg0, 0x1::vector::length<u64>(&arg0) - 1);
            let v2 = *0x1::vector::borrow<u64>(&arg1, 0x1::vector::length<u64>(&arg1) - 1);
            let v3 = if (v1 < v2) {
                0x1::vector::pop_back<u64>(&mut arg0)
            } else if (v2 < v1) {
                0x1::vector::pop_back<u64>(&mut arg1)
            } else {
                break
            };
            0x1::vector::push_back<u64>(&mut v0, v3);
        };
        v0
    }

    fun test1(arg0: &R) : u64 {
        arg0.x
    }

    fun test_if(arg0: u8, arg1: u8) : u8 {
        if (arg0 > arg1) {
            let v0 = arg0 - arg1;
            let v1 = v0;
            if (v0 > 10) {
                v1 = 0 - v0;
            };
            return v1
        };
        arg1
    }

    fun test_ints(arg0: u8) : u32 {
        (((arg0 as u16) + 1) as u32) + 2 + 3
    }

    fun test_ref(arg0: &u8, arg1: &u8) : u8 {
        if (*arg0 > *arg1) {
            let v0 = *arg0 - *arg1;
            let v1 = v0;
            if (v0 > 10) {
                v1 = 0 - v0;
            };
            return v1
        };
        *arg1
    }

    fun test_ref_mut(arg0: u8) : u8 {
        let v0 = &mut arg0;
        let v1 = 4;
        *v0 = 3;
        if (v0 == &v1) {
            *v0 = 4;
        };
        arg0
    }

    public fun test_swap(arg0: u8, arg1: u8) : u8 {
        if (arg0 > arg1) {
            let v0 = arg1;
            arg1 = arg0;
            arg0 = v0;
        };
        arg1 - arg0
    }

    public fun test_vector(arg0: u64) : u64 {
        let v0 = 0;
        let v1 = vector[1, 2, 3, 4, 5, 6, 7, 8, 9];
        while (!0x1::vector::is_empty<u64>(&v1)) {
            v0 = v0 + 0x1::vector::pop_back<u64>(&mut v1) * arg0;
        };
        v0
    }

    fun test_while(arg0: u8, arg1: u8) : u8 {
        while (arg0 < arg1) {
            if (arg0 == 9) {
                return arg1
            };
            if (arg0 == 7) {
                break
            };
            let v0 = if (arg0 > arg1 && (arg0 - arg1 * 2) / (arg1 - arg0 * 3) < arg0 + arg1) {
                arg0 - arg1
            } else {
                arg1 - arg0
            };
            let v1 = v0;
            if (arg0 == 8) {
                continue
            };
            while (v1 > 10) {
                let v2 = v1 - 1;
                v1 = v2;
                if (v2 % 2 == 3) {
                    break
                };
            };
            let v3 = arg0 + 2;
            arg0 = v3;
            if (v1 == 0 - 12) {
                return v1 - v3
            };
        };
        while (arg0 < arg1) {
        };
        77
    }

    // decompiled from Move bytecode v6
}
