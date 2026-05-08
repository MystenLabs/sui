// Test that an inner #[deny(lint(public_entry))] overrides an outer
// #[allow(lint(all))], upgrading the specific lint to an error.
#[allow(lint(all))]
module a::m {
    #[deny(lint(public_entry))]
    public entry fun foo() {}
}
