// #[test_only]
module prover::prover {
    native public fun requires(p: bool);
    native public fun ensures(p: bool);
    // native public fun invariant_(p: bool);

    public fun implies(p: bool, q: bool): bool {
        !p || q
    }

    public macro fun specs<$T>($call: &$T, $requires: ||, $ensures: |&$T|, $aborts_if: |&$T|) {
        requires_begin();
        $requires();
        requires_end();
        let result = $call;
        ensures_begin();
        $ensures(result);
        ensures_end();
        aborts_begin();
        $aborts_if(result);
        aborts_end();
    }

    public macro fun spec3<$T0, $T1, $T2, $R>($call: &$R, $a0: &$T0, $a1: &$T1, $a2: &$T2, $requires: ||, $ensures: |&$T0, &$T1, &$T2, &$R|, $aborts_if: |&$T0, &$T1, &$T2, &$R|) {
        requires_begin();
        $requires();
        requires_end();
        let a0 = $a0;
        let a1 = $a1;
        let a2 = $a2;
        let old_a0 = old!(a0);
        let old_a1 = old!(a1);
        let old_a2 = old!(a2);
        let result = $call;
        ensures_begin();
        $ensures(old_a0, old_a1, old_a2, result);
        ensures_end();
        aborts_begin();
        $aborts_if(old_a0, old_a1, old_a2, result);
        aborts_end();
    }

    public macro fun requires_block($requires: ||) {
        requires_begin();
        $requires();
        requires_end();
    }

    public macro fun ensures_block($ensures: ||) {
        ensures_begin();
        $ensures();
        ensures_end();
    }

    public macro fun aborts_if_block($aborts_if: ||) {
        aborts_begin();
        $aborts_if();
        aborts_end();
    }

    native public fun requires_begin();
    native public fun requires_end();
    native public fun ensures_begin();
    native public fun ensures_end();
    native public fun aborts_begin();
    native public fun aborts_end();

    public fun asserts(p: bool) {
        ensures(p);
    }

    native public fun val<T>(x: &T): T;
    native public fun ref<T>(x: T): &T;
    native public fun drop<T>(x: T);
    public macro fun old<$T>($x: &$T): &$T {
        ref(val($x))
    }

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
