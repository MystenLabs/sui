module Completion::colon_colon {

    public struct SomeStruct has drop {}

    public enum SomeEnum has drop{
        SomeVariant,
        SomeOtherVariant,
        NamedVariant { name1: u64, name2: u64},
        PositionalVariant(u64, u64),
    }

    public struct CompletionStruct has drop {}

    public fun foo(param1: u64, param2: u64) {}

    public fun complete_chains() {
        use Completion::colon_colon as CC;
        use Completion::colon_colon::SomeEnum as SE;
        let _local = Completion::colon_colon::SomeEnum::SomeVariant;
        ::Completion::colon_colon::SomeEnum::SomeVariant;
        Completion::dot::shadowed();
        0xCAFE::colon_colon::SomeEnum::SomeVariant;
        option::none<u64>();
        std::hash::sha2_256(vector::empty());
        CC::SomeEnum::SomeVariant;
        CC::foo(7, 42);
        SE::SomeVariant;
        SE::PositionalVariant(7, 42);
        SE::NamedVariant{name1: 7, name2: 42};
    }

    public fun single_ident() {
        std::string::
    }

    public fun mod_single_ident() {
        use Completion::colon_colon as CC;
        C
    }

    public fun member_single_ident() {
        Some
    }
}
