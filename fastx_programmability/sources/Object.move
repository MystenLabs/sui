module FastX::Object {
    use FastX::Authenticator::Authenticator;
    use FastX::ID::ID;
    use FastX::Transfer;

    /// Wrapper object for storing arbitrary mutable `data` in the global
    /// object pool.
    struct Object<T: store> has key, store {
        id: ID,
        /// Abritrary data associated with the object
        data: T
    }

    /// Create a new object wrapping `data` with id `ID`
    public fun new<T: store>(data: T, id: ID): Object<T> {
        Object { id, data }
    }

    /// Transfer object `o` to `recipient`
    public fun transfer<T: store>(o: Object<T>, recipient: Authenticator) {
        Transfer::transfer(o, recipient)
    }

    /// Get a mutable reference to the data embedded in `self`
    public fun data_mut<T: store>(self: &mut Object<T>): &mut T {
        &mut self.data
    }

    /// Get an immutable reference to the data embedded in `self`
    public fun data<T: store>(self: &Object<T>): &T {
        &self.data
    }
}
