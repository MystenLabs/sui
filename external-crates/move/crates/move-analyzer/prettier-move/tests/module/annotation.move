// options:
// printWidth: 20

module prettier::members {
    #[allow(unused_const, unused_variable)]
    #[expected_error(abort_code = ::ENotImplemented)]
    #[test, expected_failure(abort_code = other_module::ENotFound)]
    #[expected_failure(arithmetic_error, location = pkg_addr::other_module)]
    #[allow(unused_const, unused_variable, unused_imports, unused_field)]
    fun call_something() {}


    #[allow(lint(share_owned)), unused_variable, unused_imports, unused_field]
    fun call_something_else() {}
}
