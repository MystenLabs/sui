/// This module defines the Result type and its methods.
module Evm::Result {
    use std::option::{Self, Option};

    /// This struct will contain either a value of type T or an error value of type E.
    struct Result<T, E> has copy, drop, store {
        value: Option<T>,
        error: Option<E>
    }
    spec Result {
        /// `Result` cannot contain both a value and an error value.
        invariant option::is_some(value) ==> option::is_none(error);
        invariant option::is_some(error) ==> option::is_none(value);
    }

    /// Return a Result containing `value`.
    public fun ok<T, E>(value: T): Result<T, E> {
        Result<T, E>{value: option::some(value), error: option::none<E>()}
    }

    /// Return a Result containing 'error'.
    public fun err<T, E>(error: E): Result<T, E> {
        Result<T, E>{value: option::none<T>(), error: option::some(error)}
    }

    /// Return true if `result` holds a value.
    public fun is_ok<T, E>(result: &Result<T, E>): bool {
        option::is_some(&result.value)
    }

    /// Return true if `result` holds an error value.
    public fun is_err<T, E>(result: &Result<T, E>): bool {
        option::is_some(&result.error)
    }

    /// Destroy `result` and extract `value`.
    public fun unwrap<T, E>(result: Result<T, E>): T {
        let Result {value, error} = result;
        option::destroy_none(error);
        option::destroy_some(value)
    }

    /// Destroy `result` and extract `error`.
    public fun unwrap_err<T, E>(result: Result<T, E>): E {
        let Result {value, error} = result;
        option::destroy_none(value);
        option::destroy_some(error)
    }
}
