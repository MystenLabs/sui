// This test is to make sure that the compiler does not crash in the following situation. One of the
// dependency modules of the same package (ExtDepError::M1) is in pre-compiled libs while the other
// (ExtDepError::M2) is being fully compiled (due to its extension existing). Additionally, the
// pre-compiled module has a public(package) function that's called from the extension. Prior to the
// fix, compiler would crash as it would try to find pre-compiled module in the fully compiled list
// of modules.

module ExtFriendError::M {
}

#[test_only]
extend module ExtDepError::M2 {
    fun ext_fun(): u64 {
        ExtDepError::M1::pkg_fun()
    }
}
