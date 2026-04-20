//# init --edition development

//# publish
module 0x42::signed_match {

    public enum SignedVal has drop {
        Small(i8),
        Medium(i32),
        Large(i64),
        None,
    }

    // Match on enum variants containing signed int fields
    public fun describe(v: SignedVal): u8 {
        match (v) {
            SignedVal::Small(_) => 1,
            SignedVal::Medium(_) => 2,
            SignedVal::Large(_) => 3,
            SignedVal::None => 0,
        }
    }

    // Extract signed value from variant
    public fun extract_or_default(v: SignedVal): i64 {
        match (v) {
            SignedVal::Small(x) => (x as i64),
            SignedVal::Medium(x) => (x as i64),
            SignedVal::Large(x) => x,
            SignedVal::None => 0i64,
        }
    }

    // Match with guards using signed int comparisons
    public fun classify(v: SignedVal): u8 {
        match (v) {
            SignedVal::Small(x) if (*x < 0i8) => 1,
            SignedVal::Small(_) => 2,
            SignedVal::Medium(x) if (*x < 0i32) => 3,
            SignedVal::Medium(_) => 4,
            SignedVal::Large(x) if (*x < 0i64) => 5,
            SignedVal::Large(_) => 6,
            SignedVal::None => 0,
        }
    }

    // Match signed integer values directly
    public fun sign_of(x: i32): i8 {
        match (x) {
            0i32 => 0i8,
            x if (*x > 0i32) => 1i8,
            _ => -1i8,
        }
    }

    public fun test_describe() {
        assert!(describe(SignedVal::Small(-1i8)) == 1, 0);
        assert!(describe(SignedVal::Medium(42i32)) == 2, 1);
        assert!(describe(SignedVal::Large(-100i64)) == 3, 2);
        assert!(describe(SignedVal::None) == 0, 3);
    }

    public fun test_extract() {
        assert!(extract_or_default(SignedVal::Small(-5i8)) == -5i64, 0);
        assert!(extract_or_default(SignedVal::Medium(100i32)) == 100i64, 1);
        assert!(extract_or_default(SignedVal::Large(-999i64)) == -999i64, 2);
        assert!(extract_or_default(SignedVal::None) == 0i64, 3);
    }

    public fun test_classify() {
        assert!(classify(SignedVal::Small(-10i8)) == 1, 0);
        assert!(classify(SignedVal::Small(10i8)) == 2, 1);
        assert!(classify(SignedVal::Medium(-1i32)) == 3, 2);
        assert!(classify(SignedVal::Medium(1i32)) == 4, 3);
        assert!(classify(SignedVal::Large(-1i64)) == 5, 4);
        assert!(classify(SignedVal::Large(1i64)) == 6, 5);
        assert!(classify(SignedVal::None) == 0, 6);
    }

    public fun test_sign_of() {
        assert!(sign_of(0i32) == 0i8, 0);
        assert!(sign_of(42i32) == 1i8, 1);
        assert!(sign_of(-42i32) == -1i8, 2);
    }
}

//# run 0x42::signed_match::test_describe

//# run 0x42::signed_match::test_extract

//# run 0x42::signed_match::test_classify

//# run 0x42::signed_match::test_sign_of
