address 0x1 {

// Non-ASCII characters in comments (e.g., ф) are also allowed.
module temp {
    /// ❤️ Also in doc comments 💝
    public fun foo() {}
    /* block
    Comment
    Γ ⊢ λ x. x : ∀α. α → α
    */
    public fun bar() {}
}

}

module a::n {
    public fun foo() { 0x1::temp::foo() }
    public fun bar() { 0x1::temp::bar() }
}
