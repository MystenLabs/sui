module Completion::colon_colon {

    public struct SomeStruct has drop {}

    public enum SomeEnum has drop{
        SomeVariant,
        SomeOtherVariant,
    }

    public struct CompletionStruct has drop {}

    public fun foo() {}

    public fun pkg_single_ident() {
        Comp
    }

    public fun mod_single_ident() {
        use Completion::colon_colon as CompCC;
        Comp
    }

    public fun member_single_ident() {
        Som
    }

//    public fun mod() {
//        Completion::
//    }

    public fun mod_ident_local() {
        Completion::co
    }

    public fun mod_ident_other() {
        Completion::in
    }

    public fun mod_ident_from_addr() {
        0xCAFE::c
    }

    public fun member() {
        Completion::colon_colon::foo();
    }

    public fun test() {
        use Completion::colon_colon as CC;
        use Completion::colon_colon::SomeEnum as SE;
        let _local1 = Completion::colon_colon::SomeEnum::SomeVariant;
        let _local2 = 0xCAFE::colon_colon::SomeEnum::SomeVariant;
        let _local3 = option::none<u64>();
        let _local4 = CC::SomeEnum::SomeVariant;
        let _local5 = SE::SomeVariant;

    }

}
