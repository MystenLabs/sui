// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Re-export types that were moved to sui_types::messages_grpc
pub use sui_types::messages_grpc::{
    ExecutedData, QuorumTransactionResponse, WaitForEffectsRequest, WaitForEffectsResponse,
};
