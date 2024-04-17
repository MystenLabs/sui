// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This module defines the Result type and its methods to represent and handle an result value
module bridge::result {

    use std::option::{Option, some, none};

    /// Result type is used for returning and propagating errors,
    /// ok represent success and containing a value, and err represent error and containing an error value.
    public struct Result<T, E> {
        ok: Option<T>,
        err: Option<E>
    }

    /// The `Result` is in an invalid state for the operation attempted.
    /// The `Result` is `Ok` while it should be `Err`.
    const ERESULT_IS_SET: u64 = 0;
    /// The `Result` is in an invalid state for the operation attempted.
    /// The `Result` is `Err` while it should be `Ok`.
    const ERESULT_NOT_SET: u64 = 1;

    /// Returns an Ok `Result`
    public fun unwrap<T, E>(self: Result<T, E>): T {
        assert!(self.is_ok(), ERESULT_NOT_SET);
        let Result {
            ok,
            err
        } = self;
        err.destroy_none();
        ok.destroy_some()
    }

    /// Returns an Err `Result`
    public fun unwrap_err<T, E>(self: Result<T, E>): E {
        assert!(self.is_err(), ERESULT_IS_SET);
        let Result {
            ok,
            err
        } = self;
        ok.destroy_none();
        err.destroy_some()
    }

    public fun ok<T,E>(result:T): Result<T,E>{
        Result{
            ok: some(result),
            err: none()
        }
    }

    public fun err<T,E>(err:E): Result<T,E>{
        Result{
            ok: none(),
            err: some(err)
        }
    }

    public fun is_ok<T, E>(self: &Result<T, E>): bool {
        self.ok.is_some()
    }

    public fun is_err<T, E>(self: &Result<T, E>): bool {
        self.err.is_some()
    }
}
