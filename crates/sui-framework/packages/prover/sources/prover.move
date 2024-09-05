// #[test_only]
module prover::prover {
    native public fun requires(p: bool);
    native public fun ensures(p: bool);
    // native public fun invariant_(p: bool);

    public fun implies(p: bool, q: bool): bool {
        !p || q
    }

    public macro fun fun_spec<$T>($fun_call: $T, $requires_spec: ||, $ensures_spec: |&$T|, $aborts_if_spec: |&$T|): $T {
        $requires_spec();
        let result = $fun_call;
        $ensures_spec(&result);
        $aborts_if_spec(&result);
        result
    }

    public macro fun ensures_($cond: bool) {
        let cond = $cond;
        ensures(cond);
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
