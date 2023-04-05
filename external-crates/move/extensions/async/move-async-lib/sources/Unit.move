/// The `Unit` type, a type which contains a singleton element.
module Async::Unit {
    native struct Unit has drop, copy, store;
    public native fun unit(): Unit;
}
