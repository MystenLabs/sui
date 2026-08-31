module CrossModuleConsts::user {
    #[test]
    fun check() {
        assert!(ConstDeps::m::base() == 101);
        assert!(ConstDeps::m::extended() == 110);
    }
}

// extends a module of the pre-compiled dependency package
#[test_only]
extend module ConstDeps::m {
    public fun extended(): u64 {
        ConstDeps::defs::MAX + ConstDeps::defs::clamp!(200)
    }
}
