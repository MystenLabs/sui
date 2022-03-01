address StarcoinFramework {

/// Module defining error codes used in Move aborts throughout the framework.
///
/// A `u64` error code is constructed from two values:
///
///  1. The *error category* which is encoded in the lower 8 bits of the code. Error categories are
///     declared in this module and are globally unique across the Diem framework. There is a limited
///     fixed set of predefined categories, and the framework is guaranteed to use those consistently.
///
///  2. The *error reason* which is encoded in the remaining 56 bits of the code. The reason is a unique
///     number relative to the module which raised the error and can be used to obtain more information about
///     the error at hand. It is mostly used for diagnosis purposes. Error reasons may change over time as the
///     framework evolves.
///
/// Rules to declare or use *error reason*:
///  1. error reason is declared as const in the user module
///  2. error reason name must start with "E", for example, const EACCOUNT_DOES_NOT_EXIST = ...
///  3. value less than 100 is reserved for general purpose and shared by all modules
///  4. don't change general purpose error reason value, it's co-related with error code in starcoin vm
///  5. self-defined error reason value must be large than 100
///  6. error reason must be used together with error category
///
module Errors {
    spec module {
        pragma verify;
        pragma aborts_if_is_strict;
    }

    /// A function to create an error from from a category and a reason.
    fun make(category: u8, reason: u64): u64 {
        (category as u64) + (reason << 8)
    }
    spec make {
        pragma opaque = true;
	pragma verify = false;
        //ensures [concrete] result == category + (reason << 8);
        aborts_if [abstract] false;
        ensures [abstract] result == category;
    }

    /// The system is in a state where the performed operation is not allowed. Example: call to a function only allowed
    /// in genesis
    const INVALID_STATE: u8 = 1;

    /// The signer of a transaction does not have the expected address for this operation. Example: a call to a function
    /// which publishes a resource under a particular address.
    const REQUIRES_ADDRESS: u8 = 2;

    /// The signer of a transaction does not have the expected  role for this operation. Example: a call to a function
    /// which requires the signer to have the role of treasury compliance.
    const REQUIRES_ROLE: u8 = 3;

    /// The signer of a transaction does not have a required capability.
    const REQUIRES_CAPABILITY: u8 = 4;

    /// A resource is required but not published. Example: access to non-existing resource.
    const NOT_PUBLISHED: u8 = 5;

    /// Attempting to publish a resource that is already published. Example: calling an initialization function
    /// twice.
    const ALREADY_PUBLISHED: u8 = 6;

    /// An argument provided to an operation is invalid. Example: a signing key has the wrong format.
    const INVALID_ARGUMENT: u8 = 7;

    /// A limit on an amount, e.g. a currency, is exceeded. Example: withdrawal of money after account limits window
    /// is exhausted.
    const LIMIT_EXCEEDED: u8 = 8;

    /// An internal error (bug) has occurred.
    const INTERNAL: u8 = 10;

    /// deprecated code
    const DEPRECATED: u8 = 11;

    /// A custom error category for extension points.
    const CUSTOM: u8 = 255;

    /// Create an error of `invalid_state`
    public fun invalid_state(reason: u64): u64 { make(INVALID_STATE, reason) }
    spec invalid_state {
        pragma opaque = true;
        aborts_if false;
        ensures result == INVALID_STATE;
    }

    /// Create an error of `requires_address`.
    public fun requires_address(reason: u64): u64 { make(REQUIRES_ADDRESS, reason) }
    spec requires_address {
        pragma opaque = true;
        aborts_if false;
        ensures result == REQUIRES_ADDRESS;
    }
    
    /// Create an error of `requires_role`.
    public fun requires_role(reason: u64): u64 { make(REQUIRES_ROLE, reason) }
    spec requires_role {
        pragma opaque = true;
        aborts_if false;
        ensures result == REQUIRES_ROLE;
    }

    /// Create an error of `requires_capability`.
    public fun requires_capability(reason: u64): u64 { make(REQUIRES_CAPABILITY, reason) }
    spec requires_capability {
        pragma opaque = true;
        aborts_if false;
        ensures result == REQUIRES_CAPABILITY;
    }

    /// Create an error of `not_published`.
    public fun not_published(reason: u64): u64 { make(NOT_PUBLISHED, reason) }
    spec not_published {
        pragma opaque = true;
        aborts_if false;
        ensures result == NOT_PUBLISHED;
    }

    /// Create an error of `already_published`.
    public fun already_published(reason: u64): u64 { make(ALREADY_PUBLISHED, reason) }
    spec already_published {
        pragma opaque = true;
        aborts_if false;
        ensures result == ALREADY_PUBLISHED;
    }

    /// Create an error of `invalid_argument`.
    public fun invalid_argument(reason: u64): u64 { make(INVALID_ARGUMENT, reason) }
    spec invalid_argument {
        pragma opaque = true;
        aborts_if false;
        ensures result == INVALID_ARGUMENT;
    }

    /// Create an error of `limit_exceeded`.
    public fun limit_exceeded(reason: u64): u64 { make(LIMIT_EXCEEDED, reason) }
    spec limit_exceeded {
        pragma opaque = true;
        aborts_if false;
        ensures result == LIMIT_EXCEEDED;
    }

    /// Create an error of `internal`.
    public fun internal(reason: u64): u64 { make(INTERNAL, reason) }
    spec internal {
        pragma opaque = true;
        aborts_if false;
        ensures result == INTERNAL;
    }

    /// Create an error of `deprecated`.
    public fun deprecated(reason: u64): u64 { make(DEPRECATED, reason) }
    spec deprecated {
        pragma opaque = true;
        aborts_if false;
        ensures result == DEPRECATED;
    }

    /// Create an error of `custom`.
    public fun custom(reason: u64): u64 { make(CUSTOM, reason) }
    spec custom {
        pragma opaque = true;
        aborts_if false;
        ensures result == CUSTOM;
    }
}

}