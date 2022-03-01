address StarcoinFramework {
/// Helper module to do u64 arith.
module Arith {
    /// split u64 to (high, low)
    public fun split_u64(i: u64): (u64, u64) {
        (i >> 32, i & 0xFFFFFFFF)
    }

    /// combine (high, low) to u64,
    /// any lower bits of `high` will be erased, any higher bits of `low` will be erased.
    public fun combine_u64(hi: u64, lo: u64): u64 {
        (hi << 32) | (lo & 0xFFFFFFFF)
    }

    /// a + b, with carry
    public fun adc(a: u64, b: u64, carry: &mut u64) : u64 {
        let (a1, a0) = split_u64(a);
        let (b1, b0) = split_u64(b);
        let (c, r0) = split_u64(a0 + b0 + *carry);
        let (c, r1) = split_u64(a1 + b1 + c);
        *carry = c;
        combine_u64(r1, r0)
    }

    /// a - b, with borrow
    public fun sbb(a: u64, b: u64, borrow: &mut u64): u64 {
        let (a1, a0) = split_u64(a);
        let (b1, b0) = split_u64(b);
        let (b, r0) = split_u64((1 << 32) + a0 - b0 - *borrow);
        let borrowed = if(b==0) {1} else {0};
        let (b, r1) = split_u64((1 << 32) + a1 - b1 - borrowed);
        *borrow = if(b==0) {1} else {0};

        combine_u64(r1, r0)
    }
}

/// Implementation u256.
module U256 {

    use StarcoinFramework::Vector;
    use StarcoinFramework::Errors;

    const WORD: u8 = 4;


    const ERR_INVALID_LENGTH: u64 = 100;
    const ERR_OVERFLOW: u64 = 200;
    /// use vector to represent data.
    /// so that we can use buildin vector ops later to construct U256.
    /// vector should always has two elements.
    struct U256 has copy, drop, store {
        /// little endian representation
        bits: vector<u64>,
    }

    spec module {
        pragma verify = false;
    }

    public fun zero(): U256 {
        from_u128(0u128)
    }

    public fun one(): U256 {
        from_u128(1u128)
    }

    public fun from_u64(v: u64): U256 {
        from_u128((v as u128))
    }

    public fun from_u128(v: u128): U256 {
        let low = ((v & 0xffffffffffffffff) as u64);
        let high = ((v >> 64) as u64);
        let bits = Vector::singleton(low);
        Vector::push_back(&mut bits, high);
        Vector::push_back(&mut bits, 0u64);
        Vector::push_back(&mut bits, 0u64);
        U256 {
            bits
        }
    }

    #[test]
    fun test_from_u128() {
        // 2^64 + 1
        let v = from_u128(18446744073709551617u128);
        assert!(*Vector::borrow(&v.bits, 0) == 1, 0);
        assert!(*Vector::borrow(&v.bits, 1) == 1, 1);
        assert!(*Vector::borrow(&v.bits, 2) == 0, 2);
        assert!(*Vector::borrow(&v.bits, 3) == 0, 3);
    }

    public fun from_big_endian(data: vector<u8>): U256 {
        // TODO: define error code.
        assert!(Vector::length(&data) <= 32, Errors::invalid_argument(ERR_INVALID_LENGTH));
        from_bytes(&data, true)
    }

    public fun from_little_endian(data: vector<u8>): U256 {
        // TODO: define error code.
        assert!(Vector::length(&data) <= 32, Errors::invalid_argument(ERR_INVALID_LENGTH));
        from_bytes(&data, false)
    }

    public fun to_u128(v: &U256): u128 {
        assert!(*Vector::borrow(&v.bits, 3) == 0, Errors::invalid_state(ERR_OVERFLOW));
        assert!(*Vector::borrow(&v.bits, 2) == 0, Errors::invalid_state(ERR_OVERFLOW));
        ((*Vector::borrow(&v.bits, 1) as u128) << 64) | (*Vector::borrow(&v.bits, 0) as u128)
    }

    #[test]
    fun test_to_u128() {
        // 2^^128 - 1
        let i = 340282366920938463463374607431768211455u128;
        let v = from_u128(i);
        assert!(to_u128(&v) == i, 128);
    }
    #[test]
    #[expected_failure]
    fun test_to_u128_overflow() {
        // 2^^128 - 1
        let i = 340282366920938463463374607431768211455u128;
        let v = from_u128(i);
        let v = add(v, one());
        to_u128(&v);
    }

    const EQUAL: u8 = 0;
    const LESS_THAN: u8 = 1;
    const GREATER_THAN: u8 = 2;

    public fun compare(a: &U256, b: &U256): u8 {
        let i = (WORD as u64);
        while (i > 0) {
            i = i - 1;
            let a_bits = *Vector::borrow(&a.bits, i);
            let b_bits = *Vector::borrow(&b.bits, i);
            if (a_bits != b_bits) {
                if (a_bits < b_bits) {
                    return LESS_THAN
                } else {
                    return GREATER_THAN
                }
            }
        };
        EQUAL
    }

    #[test]
    fun test_compare() {
        let a = from_u64(111);
        let b = from_u64(111);
        let c = from_u64(112);
        let d = from_u64(110);
        assert!(compare(&a, &b) == EQUAL, 0);
        assert!(compare(&a, &c) == LESS_THAN, 1);
        assert!(compare(&a, &d) == GREATER_THAN, 2);
    }


    public fun add(a: U256, b: U256): U256 {
        native_add(&mut a, &b);
        a
    }

    #[test]
    fun test_add() {
        let a = Self::one();
        let b = Self::from_u128(10);
        let ret = Self::add(a, b);
        assert!(compare(&ret, &from_u64(11)) == EQUAL, 0);
    }

    public fun sub(a: U256, b: U256): U256 {
        native_sub(&mut a, &b);
        a
    }

    #[test]
    #[expected_failure]
    fun test_sub_overflow() {
        let a = Self::one();
        let b = Self::from_u128(10);
        let _ = Self::sub(a, b);
    }

    #[test]
    fun test_sub_ok() {
        let a = Self::from_u128(10);
        let b = Self::one();
        let ret = Self::sub(a, b);
        assert!(compare(&ret, &from_u64(9)) == EQUAL, 0);
    }

    public fun mul(a: U256, b: U256): U256 {
        native_mul(&mut a, &b);
        a
    }

    #[test]
    fun test_mul() {
        let a = Self::from_u128(10);
        let b = Self::from_u64(10);
        let ret = Self::mul(a, b);
        assert!(compare(&ret, &from_u64(100)) == EQUAL, 0);
    }

    public fun div(a: U256, b: U256): U256 {
        native_div(&mut a, &b);
        a
    }

    #[test]
    fun test_div() {
        let a = Self::from_u128(10);
        let b = Self::from_u64(2);
        let c = Self::from_u64(3);
        // as U256 cannot be implicitly copied, we need to add copy keyword.
        assert!(compare(&Self::div(copy a, b), &from_u64(5)) == EQUAL, 0);
        assert!(compare(&Self::div(copy a, c), &from_u64(3)) == EQUAL, 0);
    }

    public fun rem(a: U256, b: U256): U256 {
        native_rem(&mut a, &b);
        a
    }

    #[test]
    fun test_rem() {
        let a = Self::from_u128(10);
        let b = Self::from_u64(2);
        let c = Self::from_u64(3);
        assert!(compare(&Self::rem(copy a, b), &from_u64(0)) == EQUAL, 0);
        assert!(compare(&Self::rem(copy a, c), &from_u64(1)) == EQUAL, 0);
    }

    public fun pow(a: U256, b: U256): U256 {
        native_pow(&mut a, &b);
        a
    }

    #[test]
    fun test_pow() {
        let a = Self::from_u128(10);
        let b = Self::from_u64(1);
        let c = Self::from_u64(2);
        let d = Self::zero();
        assert!(compare(&Self::pow(copy a, b), &from_u64(10)) == EQUAL, 0);
        assert!(compare(&Self::pow(copy a, c), &from_u64(100)) == EQUAL, 0);
        assert!(compare(&Self::pow(copy a, d), &from_u64(1)) == EQUAL, 0);
    }

    /// move implementation of native_add.
    fun add_nocarry(a: &mut U256, b: &U256) {
        let carry = 0;
        let idx = 0;
        let len = (WORD as u64);
        while (idx < len) {
            let a_bit = Vector::borrow_mut(&mut a.bits, idx);
            let b_bit = Vector::borrow(&b.bits, idx);
            *a_bit = StarcoinFramework::Arith::adc(*a_bit, *b_bit, &mut carry);
            idx = idx + 1;
        };

        // check overflow
        assert!(carry == 0, 100);
    }

    /// move implementation of native_sub.
    fun sub_noborrow(a: &mut U256, b: &U256) {
        let borrow = 0;
        let idx = 0;
        let len =(WORD as u64);
        while (idx < len) {
            let a_bit = Vector::borrow_mut(&mut a.bits, idx);
            let b_bit = Vector::borrow(&b.bits, idx);
            *a_bit = StarcoinFramework::Arith::sbb(*a_bit, *b_bit, &mut borrow);
            idx = idx + 1;
        };

        // check overflow
        assert!(borrow == 0, 100);

    }

    native fun from_bytes(data: &vector<u8>, be: bool): U256;
    native fun native_add(a: &mut U256, b: &U256);
    native fun native_sub(a: &mut U256, b: &U256);
    native fun native_mul(a: &mut U256, b: &U256);
    native fun native_div(a: &mut U256, b: &U256);
    native fun native_rem(a: &mut U256, b: &U256);
    native fun native_pow(a: &mut U256, b: &U256);

    spec fun value_of_U256(a: U256): num {
        ( a.bits[0]             // 0 * 64
          + a.bits[1] << 64     // 1 * 64
          + a.bits[2] << 128    // 2 * 64
          + a.bits[3] << 192    // 3 * 64
        )
    }

    spec from_u128 {
        pragma opaque;
        ensures value_of_U256(result) == v;
    }

    spec to_u128 {
        pragma opaque;
        aborts_if value_of_U256(v) >= (1 << 128);
        ensures value_of_U256(v) == result;
    }

    spec add {
        pragma opaque;
        // TODO: mvp doesn't seem to be using these specs
        aborts_if value_of_U256(a) + value_of_U256(b) >= (1 << 256);
        ensures value_of_U256(result) == value_of_U256(a) + value_of_U256(b);
    }

    spec sub {
        pragma opaque;
        // TODO: mvp doesn't seem to be using these specs
        aborts_if value_of_U256(a) > value_of_U256(b);
        ensures value_of_U256(result) == value_of_U256(a) - value_of_U256(b);
    }

    spec mul {
        pragma opaque;
        // TODO: mvp doesn't seem to be using these specs
        aborts_if value_of_U256(a) * value_of_U256(b) >= (1 << 256);
        ensures value_of_U256(result) == value_of_U256(a) * value_of_U256(b);
    }

    spec div {
        pragma opaque;
        // TODO: mvp doesn't seem to be using these specs
        aborts_if value_of_U256(b) == 0;
        ensures value_of_U256(result) == value_of_U256(a) / value_of_U256(b);
    }

    spec rem {
        pragma opaque;
        // TODO: mvp doesn't seem to be using these specs
        aborts_if value_of_U256(b) == 0;
        ensures value_of_U256(result) == value_of_U256(a) % value_of_U256(b);
    }

    spec pow {
        pragma opaque;
        // TODO: mvp doesn't seem to be using these specs
        // aborts_if value_of_U256(a) * value_of_U256(b) >= (1 << 256);
        // ensures value_of_U256(result) == value_of_U256(a) / value_of_U256(b);
    }
}
}