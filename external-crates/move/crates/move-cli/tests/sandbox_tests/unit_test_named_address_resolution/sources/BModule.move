module B::BModule {
    #[allow(unused_const)]
    #[error]
    const EIsThree: vector<u8> = b"EIsThree";

    public fun abort_() {
        abort EIsThree 
    }
}
