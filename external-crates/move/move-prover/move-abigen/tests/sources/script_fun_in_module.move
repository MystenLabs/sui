address 0x1 {
module ScriptFunInModule {
    use std::string::String;

    struct NoCall has drop {}

    /// This is a doc comment on this script fun
    public entry fun this_is_a_script_fun(this_is_an_arg: u64, _another_arg: address) {
        abort this_is_an_arg
    }

    /// This is another doc comment on a different script fun
    public entry fun this_is_a_different_script_fun(this_is_an_arg: u64, _another_arg: address) {
        abort this_is_an_arg
    }

    /// This is a comment on a non-callable script function
    public entry fun this_is_a_noncallable_script_fun(): u64 {
        5
    }

    /// This is a comment on a non-callable script function
    public entry fun this_is_another_noncallable_script_fun(_blank: NoCall) { }

    /// This is a comment on a non-callable script function
    public entry fun this_is_script_fun_with_signer_ref(account: &signer, _another_arg: u64) { }

    /// This is a comment on a non-callable script function
    public entry fun this_is_script_fun_with_signer(account: signer, _another_arg: u64) { }

    /// This is a comment on a non-callable script function
    public entry fun this_is_script_fun_with_string_args(account: &signer, _val: String) { }
    public fun foo() { }

    fun bar() { }
}
}
