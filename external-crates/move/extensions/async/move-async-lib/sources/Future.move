// NOTE: this module is not implemented yet
module Async::Future {
    use Async::Unit::{Unit, Self};

    /// A type which represents a future.
    native struct Future<phantom T>;

    /// A type which represents a continuation.
    native struct Cont<phantom T, phantom R>;

    /// Yield execution to the given continuation.
    public native fun yield<T>(result: T): Future<T>;

    /// Yield a future which first executes `f`, then continues with `cont`.
    public native fun followed_by<T, R>(f: Future<T>, c: Cont<T, R>): Future<R>;

    /// Shortcut for yield(unit())
    public fun done(): Future<Unit> {
        yield(Unit::unit())
    }
}
