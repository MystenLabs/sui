module a::m {

    public struct SomeStruct has drop {
        some_field: u64
    }

    public struct AnotherStruct has drop {
        another_field: SomeStruct
    }

    fun foo(s: AnotherStruct) {
        let _tmp1 = s.;                // incomplete with `;` (next line should parse)
        let _tmp2 = s.another_field.;  // incomplete with `;` (next line should parse)
        let _tmp3 = s.another_field.   // incomplete without `;` (unexpected `let`)
        let _tmp4 = s;
        let _tmp = s.                  // incomplete without `;` (unexpected `}`)
    }
}
