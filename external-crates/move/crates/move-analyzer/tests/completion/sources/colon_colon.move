module Completion::colon_colon {

    const SOME_CONST: u64 = 42;

    public struct SomeStruct has drop {}

    public enum SomeEnum has drop {
        SomeVariant,
        SomeNamedVariant { name1: u64, name2: u64},
        SomePositionalVariant(u64, u64),
    }

    public struct CompletionStruct has drop {}

    public fun sbar(_param1: u64, _param2: Completion::colon_colon::SomeStruct) {}
    public fun sbaz() {}

    public fun complete_chains(s: SomeStruct) {
        use Completion::colon_colon as CC;
        use Completion::colon_colon::SomeEnum as SE;
        let _local = Completion::colon_colon::SomeEnum::SomeVariant;
        ::Completion::colon_colon::SomeEnum::SomeVariant;
        Completion::dot::shadowed();
        0xCAFE::colon_colon::SomeEnum::SomeVariant;
        option::none<u64>();
        std::hash::sha2_256(vector::empty());
        CC::SomeEnum::SomeVariant;
        CC::sbar(42, s);
        SE::SomeVariant;
        SE::SomePositionalVariant(7, 42);
        SE::SomeNamedVariant{name1: 7, name2: 42};

        let _struct_vec: vector<CC::SomeStruct> = vector::empty();
        let _prim_vec: vector<u64> = vector::empty();
    }

    public fun single_ident() {
        C
    }
    public fun one_colon_colon() {
        Completion::
    }
    public fun multi_colon_colon() {
        Completion::colon_colon::SomeEnum::
    }

    public enum TargEnum<T: drop> has drop {
        Variant{field: T}
    }

    public fun targ_chain() {
        use Completion::colon_colon as CC;
        // to test that variant for enums with explict type arguments auto-complete correctly
        // and that type argument auto-completes correctly
        CC::TargEnum<CC::SomeStruct>::Variant{field: CC::SomeStruct{}};
    }

    public fun targ_type<SOME_TYPE>(p: SOME_TYPE) {
        let _local: SOME_TYPE = p;
    }

    #[test, expected_failure(abort_code = Self::SOME_CONST)]
    public fun attr_chain() {
        abort(SOME_CONST);
    }

    public enum EnumWithMultiFieldVariant has drop {
        MultiFieldVariant { name1: u64, name2: u64, name3: u64},
    }

    public struct MultiFieldStruct has drop {
        field1: u64,
        field2: u64,
        field3: u64,
    }

    public fun multi_field_variant() {
        EnumWithMultiFieldVariant::
    }

    public fun multi_field_struct() {
        MultiField
    }

    public fun match_pattern(e: SomeEnum) {
        match (e) {
            SomeEnum::
        }
    }
}
