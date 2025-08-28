/// This module defines a minimal and generic Coin and Balance.
module NamedAddr::BasicCoin {

    /// Error codes
    const ENOT_MODULE_OWNER: u64 = 0;
    const EINSUFFICIENT_BALANCE: u64 = 1;
    const EALREADY_HAS_BALANCE: u64 = 2;

    struct Coin<phantom CoinType> has store {
        value: u64
    }

    struct Coin2<phantom CoinType> has store {
        value: u64,
        value2: u64
    }

    struct Balance<phantom CoinType> has key {
        coin: Coin<CoinType>
    }

    use std::vector;

    public fun test_vector(x: u64): u64 {
        let r = 0;
        let v = vector[1,2,3,4,5,6,7,8,9];
        while (!vector::is_empty(&v)) {
            let y = vector::pop_back(&mut v);
            r = r + y * x;
        };
        r
    }

    fun pop_smallest_while_not_equal(
        v1: vector<u64>,
        v2: vector<u64>,
    ): vector<u64> {
        let result = vector::empty();
        while (!vector::is_empty(&v1) && !vector::is_empty(&v2)) {
            let u1 = *vector::borrow(&v1, vector::length(&v1) - 1);
            let u2 = *vector::borrow(&v2, vector::length(&v2) - 1);
            let popped =
                if (u1 < u2) vector::pop_back(&mut v1)
                else if (u2 < u1) vector::pop_back(&mut v2)
                else break; // Here, `break` has type `u64`
            vector::push_back(&mut result, popped);
        };

        result
    }

    fun test_ref_mut(a: u8): u8 {
        let c = &mut a;
        let b = 4;
        *c = 3;
        if (c == &b) {
            *c = 4;
        };
        a
    }

    fun test_ref(a: &u8, b: &u8): u8 {
        let c = if (*a > *b) {
            *a - *b
        } else {
            return *b;
            *b - *a
        };
        if (c > 10) {
            c = 0 - c;
        };
        c
    }

    fun test_if(a: u8, b: u8): u8 {
        let c = if (a > b) {
            a - b
        } else {
            return b;
            b - a
        };
        if (c > 10) {
            c = 0 - c;
        };
        c
    }

    fun test_while(a: u8, b: u8): u8 {
        while (a < b) {
            if (a==9) {
                return b
            };
            if (a==7) {
                break
            };
            let c = if ((a > b) && (a-b*2)/(b-a*3) < a+b) {
                a - b
            } else {
                b - a
            };
            if (a==8) {
                continue
            };
            while (c > 10) {
                c = c - 1;
                if (c % 2 == 3) {
                    break
                };
            };
            a = a + 2;
            if (c == 0-12) {
                return c-a
            };
        };
        while (a < b) {};
        77
    }

    public fun test_swap(a: u8, b: u8): u8 {
        if (a>b) {
            (a,b) = (b,a);
        };
        b-a
    }

    fun test_ints(a: u8): u32 {
        let x: u16 = (a as u16)+1;
        let y: u32 = (x as u32)+2;
        y+3
    }

    struct R has copy, drop {
        x: u64
    }

    fun test1(r_ref: &R) : u64 {
        let x_ref = & r_ref.x;
        *x_ref
    }
}
