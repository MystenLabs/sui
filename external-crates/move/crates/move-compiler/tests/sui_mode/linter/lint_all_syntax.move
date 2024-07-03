// lint_allow syntax is also valid
#[lint_allow(share_owned)]
module a::m {
    #[lint_allow(all)]
    public fun foo() {}
}
