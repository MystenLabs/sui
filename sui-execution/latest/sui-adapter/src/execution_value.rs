// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::storage::{RuntimeObjectResolver, Storage};

/// Interface with the store necessary to execute a programmable transaction
pub trait ExecutionState: Storage + RuntimeObjectResolver {}

impl<T> ExecutionState for T where T: Storage + RuntimeObjectResolver {}
