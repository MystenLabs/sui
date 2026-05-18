// Test that #[warn(lint(public_entry))] adds a "lint level defined here" note
// to the sui linter diagnostic.
module a::m {
    #[warn(lint(public_entry))]
    public entry fun foo() {}
}
