// Test that #[deny(lint(public_entry))] upgrades the sui lint warning to an error.
module a::m {
    #[deny(lint(public_entry))]
    public entry fun foo() {}
}
