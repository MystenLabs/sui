// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// This standalone comment is before init.


// This comment belongs to init.
//# init --edition 2024 --addresses A=0x42 B=0x43 C=0x44 D=0x45

//# publish
// This comment belongs to the publish task and should not appear in snapshot output.
/// This Move doc comment must not be standalone snapshot output.
module A::M {
    // This Move comment must not be standalone snapshot output.
    public fun answer(): u64 {
        42
    }
}

// This comment after the published module is preserved in snapshot output.

//# publish
// This comment belongs to the publish task and should not appear in snapshot output.
/// This Move doc comment must not be standalone snapshot output.
module B::N {
    // This Move comment must not be standalone snapshot output.
    public fun noop() {}
}

// This comment after the published module is preserved in snapshot output.

//# publish
module C::Functions;
fun foo() {}

// This comment after a semicolon-header function module is preserved in snapshot output.

//# publish
module D::Constants;
const A: u64 = 10;

// This comment after a semicolon-header constant module is preserved in snapshot output.

// This comment belongs to the first run task.
//# run A::M::answer

// This standalone block is preserved.
//
// Its comment-only line is preserved.


// This is a separate standalone block.

// This comment belongs to the second run task.
//# run A::M::answer

// This trailing standalone comment is preserved.
