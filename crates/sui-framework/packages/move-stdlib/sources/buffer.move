// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Implements a temporary potato-buffer which enables glueing the contents
/// within a single transaction with the validity check when the buffer is
/// unwrapped.
///
/// Is useful when:
/// - collecting and aggregating the data from multiple calls
/// - creating a combined argument that exceeds the allowed size of a single
/// pure transaction argument (multiple pure vectors into one)
module std::buffer {
    use std::vector;

    /// Attempt to unwrap the buffer while the size of the contents does not
    /// match the expected size.
    const ESizeMismatch: u64 = 0;

    /// Generic Buffer for any type of content.
    struct Buffer<T> {
        /// Marks the expected size of the buffer.
        expected_size: u64,
        /// Stores the contents of the buffer. The size of the contents vector
        /// must match the `expected_size` on unwrap.
        contents: vector<T>
    }

    /// Create a new `Buffer` with the expected size.
    public fun new<T>(expected_size: u64): Buffer<T> {
        Buffer {
            expected_size,
            contents: vector[]
        }
    }

    /// Append data to the buffer.
    public fun append<T>(self: &mut Buffer<T>, data: vector<T>) {
        vector::append(&mut self.contents, data)
    }

    /// Unwrap the buffer and return the contents.
    public fun unwrap<T>(self: Buffer<T>): vector<T> {
        let Buffer { expected_size, contents } = self;
        assert!(vector::length(&contents) == expected_size, ESizeMismatch);
        contents
    }
}
