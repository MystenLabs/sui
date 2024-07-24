module Completion::colon_colon {

    public struct SomeStruct has drop {}

    public enum SomeEnum has drop{
        SomeVariant,
        SomeOtherVariant,
    }

    public struct CompletionStruct has drop {}

    public fun foo() {}

    public fun complete_chains() {
        use Completion::colon_colon as CC;
        use Completion::colon_colon::SomeEnum as SE;
        let _local1 = Completion::colon_colon::SomeEnum::SomeVariant;
        let _local2 = ::Completion::colon_colon::SomeEnum::SomeVariant;
        Completion::dot::shadowed();
        let _local3 = 0xCAFE::colon_colon::SomeEnum::SomeVariant;
        let _local4 = option::none<u64>();
        let _local5 = std::hash::sha2_256(vector::empty());
        let _local6 = CC::SomeEnum::SomeVariant;
        let _local7 = SE::SomeVariant;
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
