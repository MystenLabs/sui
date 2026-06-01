// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Generated protobuf types backing `sui-rpc-store` values.
//!
//! Regenerate with `cargo +nightly -Zscript codegen.rs` from the
//! crate root; see `codegen.rs` for details.

#[allow(clippy::all)]
pub mod sui {
    pub mod rpc_store {
        pub mod v1alpha {
            include!("generated/sui.rpc_store.v1alpha.rs");
            include!("generated/sui.rpc_store.v1alpha.accessors.rs");
        }
    }
}

pub use sui::rpc_store::v1alpha::BalanceDelta;
pub use sui::rpc_store::v1alpha::BitmapBlob;
pub use sui::rpc_store::v1alpha::PackageVersionInfo;
pub use sui::rpc_store::v1alpha::PruningWatermarks;
pub use sui::rpc_store::v1alpha::StoredCheckpointContents;
pub use sui::rpc_store::v1alpha::StoredCheckpointSummary;
pub use sui::rpc_store::v1alpha::StoredEffects;
pub use sui::rpc_store::v1alpha::StoredEpoch;
pub use sui::rpc_store::v1alpha::StoredEvents;
pub use sui::rpc_store::v1alpha::StoredObject;
pub use sui::rpc_store::v1alpha::StoredTransaction;
pub use sui::rpc_store::v1alpha::TxMetadata;
