module a::m {

    #[allow(lint(public_entry))]
    public entry fun suppress_unnecessary_public_entry() {}

    #[allow(lint(public_entry))]
    entry public fun suppress_unnecessary_public_entry_2() {}
}
