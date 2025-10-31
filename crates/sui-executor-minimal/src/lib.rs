// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod executor;
pub mod storage;

pub use executor::{ExecutionResult, MinimalExecutor};
pub use storage::InMemoryObjectStore;

pub use sui_execution;
pub use sui_protocol_config::ProtocolConfig;
pub use sui_types;

use sui_framework::BuiltInFramework;
use sui_types::object::Object;

pub fn genesis_packages() -> impl Iterator<Item = Object> {
    BuiltInFramework::genesis_objects()
}
