/// Actor support functions.
module Async::Actor {
    /// Returns the address of the executing actor.
    public native fun self(): address;

    /// Returns the current virtual time, in micro seconds. This time does not increase during handling
    /// of a message. On blockchains, this might be for example the block timestamp.
    public native fun virtual_time(): u128;
}
