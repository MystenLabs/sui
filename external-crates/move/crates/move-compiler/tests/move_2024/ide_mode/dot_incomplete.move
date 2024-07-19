#[allow(ide_path_autocomplete)]
module a::m {

    public struct A has copy, drop {
        x: u64
    }

    public struct B has copy, drop {
        a: A
    }

    public fun foo() {
        let _s = B { a: A { x: 0 } };
        let _tmp1 = _s.;                // incomplete with `;` (next line should parse)
        let _tmp2 = _s.a.;  // incomplete with `;` (next line should parse)
        let _tmp3 = _s.a.   // incomplete without `;` (unexpected `let`)
        let _tmp4 = _s.
        let _tmp5 = _s.                  // incomplete without `;` (unexpected `}`)
    }
}
