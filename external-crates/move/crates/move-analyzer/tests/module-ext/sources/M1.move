module ModuleExt::M1 {

    #[test]
    fun test() {
        ModuleExt::M2::bar();
        ModuleExt::M1::baz();
        Enums::int_match::bam();
    }
}

#[test_only]
extend module ModuleExt::M2 {
    public fun bar(): u64 {
        use Enums::int_match::int_match as int_match_function;

        let num = 7;
        int_match_function(num);
        num
    }
}

#[test_only]
extend module ModuleExt::M1 {
    public fun baz(): u64 {
        use Enums::int_match::int_match as int_match_function;

        let num = 7;
        int_match_function(num);
        7
    }
}

#[test_only]
extend module Enums::int_match {
    public fun bam(): u64 {
        use Enums::int_match::int_match as int_match_function;

        let num = 7;
        int_match_function(num);
        7
    }
}
