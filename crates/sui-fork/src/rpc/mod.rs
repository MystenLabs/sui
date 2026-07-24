// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! gRPC glue that adapts the forked-network primitives to the
//! `sui-rpc-api` services.

pub(crate) mod executor;
pub(crate) mod forking_service;

#[cfg(test)]
#[path = "../tests/rpc_executor.rs"]
mod executor_tests;

#[cfg(test)]
#[path = "../tests/subscription_e2e.rs"]
mod subscription_tests;
