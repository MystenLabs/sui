module PartialDot::M1 {
    public struct SomeStruct has drop {
        some_field: u64
    }

    public struct AnotherStruct has drop {
        another_field: SomeStruct
    }

    fun foo(s: AnotherStruct) {
        let _tmp1 = s.;
        let _tmp2 = s.another_field.;
        let _tmp3 = s.another_field.
        let _tmp4 = s; // statement skipped due to unexpected `let`
        let _tmp5 = s.another_field.(7, 42);
        let _tmp6 = s.
    }
}
