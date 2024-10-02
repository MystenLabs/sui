module InlayHints::param_hints {

    public struct SomeStruct has drop {
        some_field: u64,
    }

    public fun foo(first_param: u64, second_param: SomeStruct) {}


    public fun test_one_line(s: SomeStruct) {
        foo(42, s);
    }

    public fun test_mulit_line(s: SomeStruct) {
        foo(s.some_field + 42,
            s);
    }



}
