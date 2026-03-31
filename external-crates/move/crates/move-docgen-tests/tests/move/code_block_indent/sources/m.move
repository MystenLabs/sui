// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// # Code Block Formatting
///
/// Indented code inside a fenced block should be preserved:
///
/// ```
/// fun example() {
///     let x = 1;
///     if (x > 0) {
///         let y = x + 1;
///     };
/// }
/// ```
///
/// Text after code block with a nested list:
///
/// - Item one
///   - Nested item
module a::m {
    /// Function with code block in doc:
    ///
    /// ```
    /// let v = vector[1, 2, 3];
    /// let sum = 0;
    /// while (!vector::is_empty(&v)) {
    ///     sum = sum + vector::pop_back(&mut v);
    /// };
    /// ```
    ///
    /// And indented code block:
    ///
    ///   ```
    ///   indented_block();
    ///   ```
    entry fun with_code_blocks() { }
}
