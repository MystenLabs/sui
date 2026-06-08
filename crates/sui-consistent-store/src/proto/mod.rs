// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Generated protobuf types backing the framework's bookkeeping
//! column families (`__watermark`, `__restore`, `__chain_id`).
//!
//! These are the on-disk wire format only. The public API for
//! framework types lives in [`crate::framework`] and uses native
//! Rust shapes (`Watermark` struct, `RestoreState` enum); the
//! native types convert to and from these messages at the
//! [`Encode`](crate::Encode) / [`Decode`](crate::Decode) boundary.
//!
//! Regenerate via the `codegen.rs` script at the crate root after
//! changing any file under `proto/`.

#[allow(clippy::all)]
pub mod sui {
    pub mod db {
        pub mod v1alpha {
            include!("generated/sui.db.v1alpha.rs");
            include!("generated/sui.db.v1alpha.accessors.rs");
        }
    }
}
