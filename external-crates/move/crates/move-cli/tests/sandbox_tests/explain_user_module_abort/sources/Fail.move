module 0x2::Fail {
    #[allow(lint(public_entry))]
    public entry fun f() {
        abort 77
    }
}
