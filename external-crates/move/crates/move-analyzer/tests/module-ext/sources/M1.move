module ModuleExt::M1 {

    #[test]
    fun test() {
        ModuleExt::M2::bar();
        ModIdentUniform::M1::baz();
    }
}

#[test_only]
extend module ModuleExt::M2 {
    public fun bar(): u64 { 7 }
}

#[test_only]
extend module ModIdentUniform::M1 {
    public fun baz(): u64 { 7 }
}
