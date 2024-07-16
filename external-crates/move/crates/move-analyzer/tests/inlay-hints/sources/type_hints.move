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

    public struct AnotherStruct<T: drop + copy> has drop, copy {
        some_field: T,
    }

    public enum SomeEnum<T: copy + drop> has drop {
        PositionalFields(u64, AnotherStruct<T>),
        NamedFields{ num: u64, s: AnotherStruct<T> },
    }

    public fun unpack_test(some_struct: SomeStruct): u64 {
        use InlayHints::type_hints::SomeEnum as SE;
        let SomeStruct { some_field } = some_struct;
        let s = AnotherStruct { some_field };
        let SomeStruct { some_field: v } = some_struct;
        let e = SE::PositionalFields(v, s);

        match (e) {
            SomeEnum::PositionalFields(num, s) => {
                num
            },
            SE::NamedFields { num: n, s } => {
                n + s.some_field
            },
        }
    }

    public enum OuterEnum<T1, T2> has drop {
        PositionalFields(T1, T2),
        NamedFields { field: T2 },
    }

    public enum InnerEnum<L, R> has drop {
        Left(L),
        Right(R),
    }

    public fun nested_match_test(e: OuterEnum<u64, InnerEnum<u64, u64>>): u64 {
        match (e) {
            OuterEnum::PositionalFields(num, InnerEnum::Left(inner_num)) => num + inner_num,
            OuterEnum::NamedFields { field: InnerEnum::Right(inner_num) } => inner_num,
            _ => 42,
        }
    }
}
