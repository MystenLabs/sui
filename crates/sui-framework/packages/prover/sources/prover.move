module prover::prover {
    use prover::ghost::Self;

    #[verify_only]
    native public fun requires(p: bool);
    #[verify_only]
    native public fun ensures(p: bool);
    #[verify_only]
    native public fun asserts(p: bool);
    #[verify_only]
    public macro fun invariant($invariants: ||) {
        invariant_begin();
        $invariants();
        invariant_end();
    }

    public fun implies(p: bool, q: bool): bool {
        !p || q
    }

    #[verify_only]
    native public fun invariant_begin();
    #[verify_only]
    native public fun invariant_end();

    #[verify_only]
    native public fun val<T>(x: &T): T;
    #[verify_only]
    fun val_spec<T>(x: &T): T {
        let result = val(x);

        ensures(result == x);

        result
    }

    #[verify_only]
    native public fun ref<T>(x: T): &T;
    #[verify_only]
    fun ref_spec<T>(x: T): &T {
        let old_x = val(&x);

        let result = ref(x);

        ensures(result == old_x);
        drop(old_x);

        result
    }

    #[verify_only]
    native public fun drop<T>(x: T);
    #[verify_only]
    fun drop_spec<T>(x: T) {
        drop(x);
    }

    #[verify_only]
    public macro fun old<$T>($x: &$T): &$T {
        ref(val($x))
    }

    #[verify_only]
    native public fun fresh<T>(): T;
    #[verify_only]
    fun fresh_spec<T>(): T {
        fresh()
    }

    #[allow(unused)]
    native fun type_inv<T>(x: &T): bool;

    const MAX_U8: u8 = 255u8;
    const MAX_U16: u16 = 65535u16;
    const MAX_U32: u32 = 4294967295u32;
    const MAX_U64: u64 = 18446744073709551615u64;
    const MAX_U128: u128 = 340282366920938463463374607431768211455u128;
    const MAX_U256: u256 = 115792089237316195423570985008687907853269984665640564039457584007913129639935u256;
    public fun max_u8(): u8 {
        MAX_U8
    }
    public fun max_u16(): u16 {
        MAX_U16
    }
    public fun max_u32(): u32 {
        MAX_U32
    }
    public fun max_u64(): u64 {
        MAX_U64
    }
    public fun max_u128(): u128 {
        MAX_U128
    }
    public fun max_u256(): u256 {
        MAX_U256
    }
}
