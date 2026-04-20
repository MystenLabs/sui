//# init --edition development

//# publish
module 0x42::signed_provider {

    public struct SignedData has copy, drop {
        val: i64,
        small: i8,
    }

    public fun get_negative(): i64 {
        -42i64
    }

    public fun get_positive(): i64 {
        100i64
    }

    public fun make_signed_data(v: i64, s: i8): SignedData {
        SignedData { val: v, small: s }
    }

    public fun get_val(d: &SignedData): i64 {
        d.val
    }

    public fun get_small(d: &SignedData): i8 {
        d.small
    }
}

//# publish
module 0x43::signed_consumer {
    use 0x42::signed_provider;

    public fun use_negative(): i64 {
        let x = signed_provider::get_negative();
        x * 2
    }

    public fun use_positive(): i64 {
        signed_provider::get_positive() + signed_provider::get_negative()
    }

    public fun use_struct(): (i64, i8) {
        let data = signed_provider::make_signed_data(-10i64, 127i8);
        (signed_provider::get_val(&data), signed_provider::get_small(&data))
    }

    public fun use_struct_negative(): (i64, i8) {
        let data = signed_provider::make_signed_data(-999i64, -128i8);
        (signed_provider::get_val(&data), signed_provider::get_small(&data))
    }
}

//# run
module 0x44::main {
    use 0x43::signed_consumer;

    fun main() {
        assert!(signed_consumer::use_negative() == -84i64, 0);
        assert!(signed_consumer::use_positive() == 58i64, 1);

        let (v, s) = signed_consumer::use_struct();
        assert!(v == -10i64, 2);
        assert!(s == 127i8, 3);

        let (v2, s2) = signed_consumer::use_struct_negative();
        assert!(v2 == -999i64, 4);
        assert!(s2 == -128i8, 5);
    }
}
