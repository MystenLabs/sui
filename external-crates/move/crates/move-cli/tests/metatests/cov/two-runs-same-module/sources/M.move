module 0x42::M {
    #[allow(lint(public_entry))]
    public entry fun test(x: u8) {
        if (x == 0) {
            return ()
        } else {
            return ()
        }
    }
}
