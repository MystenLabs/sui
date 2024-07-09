module InlayHints::type_hints {

    public struct SomeStruct has drop, copy {
        some_field: u64,
    }

    public fun local_hints(s: SomeStruct) {
        let prim_local = 42;
        let struct_local = s;
    }

    macro fun foo($i: u64, $body: |u64| -> u64): u64 {
        $body($i)
    }

    macro fun bar($i: SomeStruct, $body: |SomeStruct| -> SomeStruct): SomeStruct {
        $body($i)
    }

    macro fun baz<$T>($i: $T, $body: |$T| -> $T): $T {
        $body($i)
    }

    public fun lambda_hints(s: SomeStruct) {
        foo!(42, |x_int| x_int);
        bar!(s, |x_struct| x_struct);
        baz!(42, |x_gen_int| x_gen_int);
        baz!(s, |x_gen_struct| x_gen_struct);
    }

}
