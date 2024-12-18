module Symbols::M10 {
   use Symbols::M9;
}

/// A module doc comment
module Symbols::M9 {

    use Symbols::M1;
    use Symbols::M1 as ALIAS_M1;
    use Symbols::{M1 as BLAH, M2 as BLEH};
    use Symbols::M2::{SomeOtherStruct as S, some_other_struct};

    const SOME_CONST: u64 = 42;

    public struct SomeStruct has drop, store {
        some_field: u64,
    }

    public fun pack(): Symbols::M9::SomeStruct  {
        Symbols::M9::SomeStruct { some_field: Symbols::M9::SOME_CONST }
    }

    public fun unpack(s: Symbols::M9::SomeStruct): u64 {
        let Symbols::M9::SomeStruct { some_field } = s;
        some_field
    }

    public fun diff_mod_struct(s: M1::SomeStruct): ALIAS_M1::SomeStruct  {
        s
    }

    public fun diff_mod_inner_use(s: M1::SomeStruct): ALIAS_M1::SomeStruct  {
        use Symbols::M1 as ANOTHER_ALIAS_M1;
        let ret: ANOTHER_ALIAS_M1::SomeStruct = Symbols::M9::diff_mod_struct(s);
        ret
    }

    public fun use_test(): S {
        some_other_struct(42)
    }
}
