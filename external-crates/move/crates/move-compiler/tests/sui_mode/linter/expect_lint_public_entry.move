// Test that #[expect(lint(public_entry))] suppresses the sui lint and the
// expectation is fulfilled.
module a::m {
    #[expect(lint(public_entry))]
    public entry fun foo() {}
}
