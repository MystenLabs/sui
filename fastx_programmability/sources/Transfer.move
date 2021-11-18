module FastX::Transfer {
    use FastX::Authenticator::Authenticator;

    /// Transfer ownership of `obj` to `recipient`. `obj` must have the
    /// `key` attribute, which (in turn) ensures that `obj` has a globally
    /// unique ID.
    // TODO: add bytecode verifier pass to ensure that `T` is a struct declared
    // in the calling module. This will allow modules to define custom transfer
    // logic for their structs that cannot be subverted by other modules
    public native fun transfer<T: key>(obj: T, recipient: Authenticator);
}
