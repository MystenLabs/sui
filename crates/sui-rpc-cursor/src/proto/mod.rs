// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Generated protobuf types backing the opaque cursor wire format.
//!
//! These are the on-wire format only. The public API is the native
//! [`CursorToken`](crate::CursorToken); it converts to and from these messages at the encode/decode
//! boundary.
//!
//! Regenerate via the `bootstrap` test (`cargo test -p sui-rpc-cursor`) after changing any file
//! under `proto/`, then commit the output.

#[allow(clippy::all)]
pub mod sui {
    pub mod rpc {
        pub mod cursor {
            pub mod v1 {
                include!("generated/sui.rpc.cursor.v1.rs");
            }
        }
    }
}
