/// Utility functions for dynamic lookup of objects
// TODO: We may not want to expose some or all of thes.
// For now, we're using dynamic lookups as a temporary
// crutch to work around the inability to pass structured
// objects (i.e., FastX objects) into the Move VM from the
// outside
module FastX::Object {
    //use FastX::ID::ID;
    //use FastX::TxContext::TxContext;

    /*/// Remove and return the object of type `T` with id `ID`.
    /// Aborts if the global object pool does not have an object
    /// named `id`.
    /// Aborts if `T.owner != ctx.sender`.
    // TODO: enforce private type restriction on `T`
    public native fun remove<T: key>(id: &ID, ctx: &TxContext): T;
     */

    // TODO: can't support borrow and borrow_mut because of dangling ref
    // risks
}
