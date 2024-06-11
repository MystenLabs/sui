// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Chain ID of the current chain
pub const X_SUI_CHAIN_ID: &str = "x-sui-chain-id";

/// Chain name of the current chain
pub const X_SUI_CHAIN: &str = "x-sui-chain";

/// Current checkpoint height
pub const X_SUI_CHECKPOINT_HEIGHT: &str = "x-sui-checkpoint-height";

/// Oldest non-pruned checkpoint height
pub const X_SUI_OLDEST_CHECKPOINT_HEIGHT: &str = "x-sui-oldest-checkpoint-height";

/// Current epoch of the chain
pub const X_SUI_EPOCH: &str = "x-sui-epoch";

/// Cursor to be used for endpoints that support cursor-based pagination. Pass this to the start field of the endpoint on the next call to get the next page of results.
pub const X_SUI_CURSOR: &str = "x-sui-cursor";

/// Current timestamp of the chain - represented as number of milliseconds from the Unix epoch
pub const X_SUI_TIMESTAMP_MS: &str = "x-sui-timestamp-ms";
