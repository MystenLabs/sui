module CrossModuleConsts::user {
    #[test]
    fun check() {
        assert!(ConstDeps::m::base() == 101);
        assert!(ConstDeps::m::extended() == 110);
    }
}

// Extending a module of the (pre-compiled) dependency package forces that one module to
// recompile from source: its constants, and the extension's, fold against the dependency
// package's pre-compiled constant values
#[test_only]
extend module ConstDeps::m {
    public fun extended(): u64 {
        ConstDeps::defs::MAX + ConstDeps::defs::clamp!(200)
    }
}
