module std::signer {
    // Borrows the address of the signer
    // Conceptually, you can think of the `signer` as being a struct wrapper arround an
    // address
    // ```
    // struct signer has drop { addr: address }
    // ```
    // `borrow_address` borrows this inner field
    native public fun borrow_address(s: &signer): &address;

    // Copies the address of the signer
    public fun address_of(s: &signer): address {
        *borrow_address(s)
    }
}
