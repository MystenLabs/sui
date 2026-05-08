// Test that an inner #[allow(lint(public_entry))] overrides an outer
// #[deny(lint(all))], suppressing the specific lint diagnostic.
#[deny(lint(all))]
module a::m {
    #[allow(lint(public_entry))]
    public entry fun foo() {}
}
