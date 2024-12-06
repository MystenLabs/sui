module a::m {

    struct SomeStruct has copy, drop {
        some_field: u64
    }

    struct AnotherStruct has copy, drop {
        another_field: SomeStruct
    }

    fun foo() {
        let s = AnotherStruct { another_field: SomeStruct { some_field: 0 } };
        let _tmp1 = s.;                // incomplete with `;` (next line should parse)
        let _tmp2 = s.another_field.;  // incomplete with `;` (next line should parse)
        let _tmp3 = s.another_field.   // incomplete without `;` (unexpected `let`)
        let _tmp4 = s;
        let _tmp = s.                  // incomplete without `;` (unexpected `}`)
    }
}
